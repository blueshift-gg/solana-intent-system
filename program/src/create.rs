use crate::events::emit;
use crate::helpers::{check_pda, create_pda, decimals, sha256};
use crate::state::Mandate;
use brine_ed25519::hasher::{FastSha512, Hasher};
use mandate_core::render::{envelope, render};
use mandate_core::{constants::*, errors::MandateError, terms::Terms, Sink};
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # Create
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
/// 2. payer:           [signer, mut]   funds the rent; refunded by `Close`
/// 3. mandate:         [mut]           PDA [MANDATE_SEED, authority, sha256(terms)]
/// 4. system_program:  [executable]
/// 5. engine:                          event signer
/// 6. program:         [executable]    this program, for the event CPI
/// 7. mints…:                          with a signature: every mint the text names
///
/// Parameters:
/// 1. terms_len: u16,
/// 2. terms: [u8; terms_len],      // canonical terms
/// 3. signature: [u8; 64],         // optional: over the OCMS v1 envelope and rendered text
///
/// Account Checks:
/// - Authority: the terms' authority; a signer when no signature is given
/// - Mandate: writable; the PDA check needs the terms, so it runs in process
/// - Payer, SystemProgram, Engine, Program: no need to check since the CPIs fail otherwise
///
/// Instruction Checks:
/// - Terms: canonical and valid
/// - Signature: only for terms that expire
///
/// Event Data:
/// - discriminator: u8, (255u8, 0u8)
/// - mandate: Pubkey,
/// - authority: Pubkey,
pub struct Create<'a> {
    pub authority: &'a AccountInfo,
    pub payer: &'a AccountInfo,
    pub mandate: &'a AccountInfo,
    pub engine: &'a AccountInfo,
    pub program: &'a AccountInfo,
    pub mints: &'a [AccountInfo],
    pub bytes: &'a [u8],
    pub terms: Terms<'a>,
    pub signature: Option<&'a [u8; 64]>,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Create<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("Create");

        let [authority, payer, mandate, _system_program, engine, program, mints @ ..] = accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        let malformed = ProgramError::InvalidInstructionData;
        let (len, rest) = data.split_first_chunk::<2>().ok_or(malformed.clone())?;
        let (bytes, rest) = rest
            .split_at_checked(u16::from_le_bytes(*len) as usize)
            .ok_or(malformed.clone())?;
        let signature: Option<&[u8; 64]> = match rest.len() {
            0 => None,
            _ => Some(rest.try_into().map_err(|_| malformed)?),
        };

        // Instruction Checks
        let terms = Terms::decode(bytes)?;
        // A mandate that never expires can be closed at once, so no signature
        // may exist that could create it again
        if signature.is_some() && terms.not_after.is_none() {
            return Err(MandateError::ExpiryRequired.into());
        }

        // Account Checks
        if terms.authority.ne(authority.key()) {
            return Err(MandateError::InvalidAuthority.into());
        }
        if signature.is_none() && !authority.is_signer() {
            return Err(MandateError::NotSigner.into());
        }
        if !mandate.is_writable() {
            return Err(MandateError::NotMutable.into());
        }

        Ok(Self {
            authority,
            payer,
            mandate,
            engine,
            program,
            mints,
            bytes,
            terms,
            signature,
        })
    }
}

impl<'a> Create<'a> {
    pub const DISCRIMINATOR: &'a u8 = &0;

    pub fn process(&mut self) -> ProgramResult {
        let authority = self.terms.authority;

        // The mandate must be the empty PDA for these exact terms. A revoked
        // mandate keeps its account, so a signature cannot create it again
        let id = sha256(self.bytes);
        let seeds: [&[u8]; 3] = [MANDATE_SEED, authority, &id];
        let bump = check_pda(self.mandate, &seeds)?;
        if self.mandate.is_owned_by(&crate::ID) {
            return Err(MandateError::AlreadyInitialized.into());
        }
        if let Some(signature) = self.signature {
            verify(&self.terms, signature, self.mints)?;
        }

        // Create it and copy the terms after the header
        let len = MANDATE_LEN + self.bytes.len();
        create_pda(self.payer, self.mandate, len, &seeds, bump)?;
        // SAFETY: the account was just created with `len` bytes, and nothing else borrows it.
        let data = unsafe { self.mandate.borrow_mut_data_unchecked() };
        data[MANDATE_LEN..len].copy_from_slice(self.bytes);
        // SAFETY: `data` holds at least the header; all fields have alignment 1.
        let mandate = unsafe { Mandate::from_bytes_unchecked_mut(data) };
        mandate.set_tag(MANDATE_TAG);
        mandate.payer = *self.payer.key();
        mandate.set_terms_len(self.bytes.len() as u16);

        // Log the Create Event
        emit(
            self.engine,
            self.program,
            *Self::DISCRIMINATOR,
            &[self.mandate.key(), authority],
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
    let decimals = |mint: &[u8; 32]| {
        let account = mints.iter().find(|m| m.key().eq(mint));
        decimals(account.ok_or(MandateError::InvalidTarget)?)
    };
    render(terms, decimals, &mut challenge)?;

    brine_ed25519::verify_prehashed_strict(
        &brine_ed25519::Address::new_from_array(*terms.authority),
        signature,
        &challenge.0.finalize(),
    )
    .map_err(|_| MandateError::InvalidSignature.into())
}
