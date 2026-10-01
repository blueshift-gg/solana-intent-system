use crate::events::emit;
use crate::helpers::{check_pda, create_pda, decimals, find, sha256};
use crate::state::{current_epoch, Load, Mandate};
use brine_ed25519::hasher::{FastSha512, Hasher};
use mandate_core::render::{envelope, render};
use mandate_core::{constants::*, errors::MandateError, terms::Terms, Sink};
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # CreateMandate
///
/// Put a mandate on chain. The authority consents by signing this
/// transaction, or by a signature over the canonical text that anyone may
/// bring: then the authority pays nothing and signs no transaction.
///
/// > Authenticate: the authority's transaction signature, or its signature over the rendered text
/// > Create the Mandate PDA for these exact terms
/// > Copy the canonical terms after its header
///
/// Accounts:
///
/// 1. authority:       [signer?]       a signer unless a signature is given
/// 2. payer:           [signer, mut]   funds the Mandate rent
/// 3. mandate:         [mut]           PDA [MANDATE_SEED, authority, sha256(terms)]
/// 4. epoch:                           PDA [EPOCH_SEED, authority], possibly absent
/// 5. system_program:  [executable]
/// 6. engine:                          event signer
/// 7. program:         [executable]    this program, for the event CPI
/// 8. mints…:                          with a signature: every mint the terms name
///
/// Parameters:
/// 1. terms_len: u16,
/// 2. terms: [u8; terms_len],      // canonical terms
/// 3. signature: [u8; 64],         // optional: over the OCMS v1 envelope and rendered text
///
/// Account Checks:
/// - Authority: the terms' authority; a signer when no signature is given
/// - Mandate: writable; the PDA check needs the terms, so it runs in process
/// - Epoch: the PDA, checked when read in process
/// - Payer, SystemProgram, Engine, Program: no need to check since the CPIs fail otherwise
///
/// Instruction Checks:
/// - Terms: canonical and valid, in the authority's current epoch
///
/// Event Data:
/// - discriminator: u8, (255u8, 0u8)
/// - mandate: Pubkey,
/// - authority: Pubkey,
pub struct CreateMandateAccounts<'a> {
    pub authority: &'a AccountInfo,
    pub payer: &'a AccountInfo,
    pub mandate: &'a AccountInfo,
    pub epoch: &'a AccountInfo,
    pub engine: &'a AccountInfo,
    pub program: &'a AccountInfo,
    pub mints: &'a [AccountInfo],
}

impl<'a> TryFrom<&'a [AccountInfo]> for CreateMandateAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [authority, payer, mandate, epoch, _system_program, engine, program, mints @ ..] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !mandate.is_writable() {
            return Err(MandateError::NotMutable.into());
        }

        // Return the accounts
        Ok(Self {
            authority,
            payer,
            mandate,
            epoch,
            engine,
            program,
            mints,
        })
    }
}

pub struct CreateMandateInstructionData<'a> {
    pub bytes: &'a [u8],
    pub terms: Terms<'a>,
    pub signature: Option<&'a [u8; 64]>,
}

impl<'a> TryFrom<&'a [u8]> for CreateMandateInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        let malformed = ProgramError::InvalidInstructionData;
        let (len, rest) = data.split_first_chunk::<2>().ok_or(malformed.clone())?;
        let (bytes, rest) = rest
            .split_at_checked(u16::from_le_bytes(*len) as usize)
            .ok_or(malformed.clone())?;
        let signature = match rest.len() {
            0 => None,
            _ => Some(rest.try_into().map_err(|_| malformed)?),
        };

        // Instruction Checks
        let terms = Terms::decode(bytes)?;

        Ok(Self {
            bytes,
            terms,
            signature,
        })
    }
}

pub struct CreateMandate<'a> {
    pub accounts: CreateMandateAccounts<'a>,
    pub instruction_data: CreateMandateInstructionData<'a>,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for CreateMandate<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("CreateMandate");

        let accounts = CreateMandateAccounts::try_from(accounts)?;
        let instruction_data = CreateMandateInstructionData::try_from(data)?;
        if instruction_data
            .terms
            .authority
            .ne(accounts.authority.key())
        {
            return Err(MandateError::InvalidAuthority.into());
        }
        if instruction_data.signature.is_none() && !accounts.authority.is_signer() {
            return Err(MandateError::NotSigner.into());
        }

        // Return the initialized struct
        Ok(Self {
            accounts,
            instruction_data,
        })
    }
}

impl<'a> CreateMandate<'a> {
    pub const DISCRIMINATOR: &'a u8 = &0;

    pub fn process(&mut self) -> ProgramResult {
        let a = &self.accounts;
        let (bytes, terms) = (self.instruction_data.bytes, &self.instruction_data.terms);
        let authority = terms.authority;

        // The mandate must be the empty PDA for these exact terms. A used or
        // revoked mandate keeps its account, so a signature cannot create it again
        let id = sha256(bytes);
        let seeds: [&[u8]; 3] = [MANDATE_SEED, authority, &id];
        let bump = check_pda(a.mandate, &seeds)?;
        if a.mandate.is_owned_by(&crate::ID) {
            return Err(MandateError::AlreadyInitialized.into());
        }

        // Only terms of the authority's current epoch
        if current_epoch(a.epoch, authority)? != terms.epoch {
            return Err(MandateError::Revoked.into());
        }
        if let Some(signature) = self.instruction_data.signature {
            verify(terms, signature, a.mints)?;
        }

        // Create it and copy the terms after the header
        let len = MANDATE_LEN + bytes.len();
        create_pda(a.payer, a.mandate, len, &seeds, bump)?;
        let mandate = Mandate::load_raw(a.mandate)?;
        mandate.set_tag(MANDATE_TAG);
        mandate.set_not_after(terms.not_after.unwrap_or(i64::MAX));
        mandate.set_epoch(terms.epoch);
        mandate.authority = *authority;
        mandate.payer = *a.payer.key();
        mandate.epoch_account = *a.epoch.key();
        mandate.set_terms_len(bytes.len() as u16);
        // SAFETY: the account was just created with room for the terms after the header.
        let data = unsafe { a.mandate.borrow_mut_data_unchecked() };
        data[MANDATE_LEN..len].copy_from_slice(bytes);

        // Log the CreateMandate Event
        emit(
            a.engine,
            a.program,
            *Self::DISCRIMINATOR,
            &[a.mandate.key(), authority],
        )
    }
}

/// Verify the authority's signature over the OCMS v1 envelope and the text
/// rendered from `terms`, streaming the render into the challenge hash.
fn verify(terms: &Terms, signature: &[u8; 64], mints: &[AccountInfo]) -> ProgramResult {
    struct Challenge(FastSha512);
    impl Sink for Challenge {
        fn put(&mut self, bytes: &[u8]) {
            self.0.update(bytes);
        }
    }

    let mut challenge = Challenge(FastSha512::new());
    challenge.put(&signature[..32]);
    challenge.put(terms.authority);
    envelope(terms.authority, &mut challenge);
    render(terms, |mint| decimals(find(mints, mint)?), &mut challenge)?;

    brine_ed25519::verify_prehashed_strict(
        &brine_ed25519::Address::new_from_array(*terms.authority),
        signature,
        &challenge.0.finalize(),
    )
    .map_err(|_| MandateError::InvalidSignature.into())
}
