use crate::events::emit;
use crate::state::{Epoch, Load};
use mandate_core::{constants::EPOCH_SEED, errors::MandateError};
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # BumpEpoch
///
/// Revoke every mandate of the authority at once, created or only signed.
///
/// > Create the epoch account if needed
/// > Increment the epoch; terms carrying an older one stop working
///
/// Accounts:
///
/// 1. authority:       [signer]
/// 2. payer:           [signer, mut]   funds the epoch rent if it is new
/// 3. epoch:           [mut]           PDA [EPOCH_SEED, authority]
/// 4. system_program:  [executable]
/// 5. engine:                          event signer
/// 6. program:         [executable]    this program, for the event CPI
///
/// Account Checks:
/// - Authority: signer
/// - Payer: no need to check since the create CPI fails otherwise
/// - Epoch: writable; the PDA, or created there
/// - SystemProgram, Engine, Program: no need to check since the CPIs fail otherwise
///
/// Event Data:
/// - discriminator: u8, (255u8, 2u8)
/// - authority: Pubkey,
/// - epoch: u32,
pub struct BumpEpoch<'a> {
    pub authority: &'a AccountInfo,
    pub payer: &'a AccountInfo,
    pub epoch: &'a AccountInfo,
    pub engine: &'a AccountInfo,
    pub program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for BumpEpoch<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        sol_log("BumpEpoch");

        let [authority, payer, epoch, _system_program, engine, program] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !authority.is_signer() {
            return Err(MandateError::NotSigner.into());
        }
        if !epoch.is_writable() {
            return Err(MandateError::NotMutable.into());
        }

        Ok(Self {
            authority,
            payer,
            epoch,
            engine,
            program,
        })
    }
}

impl<'a> BumpEpoch<'a> {
    pub const DISCRIMINATOR: &'a u8 = &2;

    pub fn process(&mut self) -> ProgramResult {
        let authority = self.authority.key();

        // Increment, creating the account on first use
        let epoch = Epoch::load_or_create(self.payer, self.epoch, &[EPOCH_SEED, authority], |e| {
            e.authority = *authority
        })?;
        if epoch.authority.ne(authority) {
            return Err(MandateError::InvalidSeeds.into());
        }
        let next = epoch.epoch().checked_add(1).ok_or(MandateError::Overflow)?;
        epoch.set_epoch(next);

        // Log the BumpEpoch Event
        emit(
            self.engine,
            self.program,
            *Self::DISCRIMINATOR,
            &[authority, &next.to_le_bytes()],
        )
    }
}
