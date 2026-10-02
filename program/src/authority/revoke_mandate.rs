use crate::events::emit;
use crate::state::{current_epoch, load_mandate, Load, Mandate};
use mandate_core::{constants::*, errors::MandateError, terms::Terms};
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # RevokeMandate
///
/// Revoke one mandate. Its authority may, whether the mandate is on chain yet
/// or only signed; so may the executor its terms name, once it is on chain.
///
/// > Create its tombstone if it was never created: a header with no terms
/// > Mark the mandate revoked
///
/// Accounts:
///
/// 1. revoker:         [signer]        the authority, or the terms' executor
/// 2. payer:           [signer, mut]   funds the tombstone rent if it is new
/// 3. mandate:         [mut]           PDA [MANDATE_SEED, authority, mandate_id]
/// 4. epoch:                           PDA [EPOCH_SEED, authority], possibly absent
/// 5. system_program:  [executable]
/// 6. engine:                          event signer
/// 7. program:         [executable]    this program, for the event CPI
///
/// Parameters:
/// 1. mandate_id: [u8; 32],    // sha256 of the canonical terms
///
/// Account Checks:
/// - Revoker: signer; the mandate's authority or its executor, checked in process
/// - Mandate: writable; the PDA, or created there
/// - Epoch: the PDA, checked when a tombstone is created
/// - Payer, SystemProgram, Engine, Program: no need to check since the CPIs fail otherwise
///
/// Event Data:
/// - discriminator: u8, (255u8, 1u8)
/// - mandate: Pubkey,
/// - revoker: Pubkey,
pub struct RevokeMandateAccounts<'a> {
    pub authority: &'a AccountInfo,
    pub payer: &'a AccountInfo,
    pub mandate: &'a AccountInfo,
    pub epoch: &'a AccountInfo,
    pub engine: &'a AccountInfo,
    pub program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for RevokeMandateAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [authority, payer, mandate, epoch, _system_program, engine, program] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !authority.is_signer() {
            return Err(MandateError::NotSigner.into());
        }
        if !mandate.is_writable() {
            return Err(MandateError::NotMutable.into());
        }

        Ok(Self {
            authority,
            payer,
            mandate,
            epoch,
            engine,
            program,
        })
    }
}

pub struct RevokeMandate<'a> {
    pub accounts: RevokeMandateAccounts<'a>,
    pub mandate_id: &'a [u8; 32],
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for RevokeMandate<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("RevokeMandate");

        Ok(Self {
            accounts: RevokeMandateAccounts::try_from(accounts)?,
            mandate_id: data
                .try_into()
                .map_err(|_| ProgramError::InvalidInstructionData)?,
        })
    }
}

impl<'a> RevokeMandate<'a> {
    pub const DISCRIMINATOR: &'a u8 = &1;

    pub fn process(&mut self) -> ProgramResult {
        let a = &self.accounts;
        let revoker = a.authority.key();

        // A mandate that was never created gets a tombstone, which only its
        // authority can make: the PDA is derived from the signer. A signature
        // for it may exist and its expiry is unknown, so it stays until the epoch moves
        if !a.mandate.is_owned_by(&crate::ID) {
            let epoch = current_epoch(a.epoch, revoker)?;
            let seeds: [&[u8]; 3] = [MANDATE_SEED, revoker, self.mandate_id];
            Mandate::load_or_create(a.payer, a.mandate, &seeds, |m| {
                m.set_not_after(i64::MAX);
                m.set_epoch(epoch);
                m.authority = *revoker;
                m.payer = *a.payer.key();
                m.epoch_account = *a.epoch.key();
            })?;
        }
        let (mandate, bytes) = load_mandate(a.mandate)?;
        let executor = || Terms::decode(bytes).ok().and_then(|t| t.executor);
        if mandate.authority.ne(revoker) && executor().is_none_or(|e| e.ne(revoker)) {
            return Err(MandateError::InvalidAuthority.into());
        }
        mandate.set_flags(mandate.flags() | REVOKED);

        // Log the RevokeMandate Event
        emit(
            a.engine,
            a.program,
            *Self::DISCRIMINATOR,
            &[a.mandate.key(), revoker],
        )
    }
}
