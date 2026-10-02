use crate::events::emit;
use crate::helpers::close;
use crate::state::{mandate_epoch, Load, Mandate};
use mandate_core::errors::MandateError;
use pinocchio::log::sol_log;
use pinocchio::sysvars::{clock::Clock, Sysvar};
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # CloseMandate
///
/// Close a mandate whose terms can never run again and return its rent to
/// whoever paid it: the window has closed, or the authority bumped its epoch.
/// A mandate is its own tombstone. Done or revoked, it still stays until
/// then: the program cannot know whether a signature for the same terms
/// exists, and closing earlier would let that signature create it again.
///
/// > Close the Mandate into its payer
///
/// Accounts:
///
/// 1. mandate:         [mut]
/// 2. payer:           [mut]           the recorded payer, receives the rent
/// 3. epoch:                           the mandate's Epoch PDA, possibly absent
/// 4. engine:                          event signer
/// 5. program:         [executable]    this program, for the event CPI
///
/// Account Checks:
/// - Mandate: writable, loaded
/// - Payer: writable, equal to mandate.payer
/// - Epoch: the account the mandate recorded, checked when read in process
/// - Engine, Program: no need to check since the event CPI fails otherwise
///
/// Event Data:
/// - discriminator: u8, (255u8, 22u8)
/// - mandate: Pubkey,
pub struct CloseMandate<'a> {
    pub mandate: &'a AccountInfo,
    pub payer: &'a AccountInfo,
    pub epoch: &'a AccountInfo,
    pub engine: &'a AccountInfo,
    pub program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for CloseMandate<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        sol_log("CloseMandate");

        let [mandate, payer, epoch, engine, program] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !mandate.is_writable() || !payer.is_writable() {
            return Err(MandateError::NotMutable.into());
        }
        if Mandate::load(mandate)?.payer.ne(payer.key()) {
            return Err(MandateError::InvalidPayer.into());
        }

        Ok(Self {
            mandate,
            payer,
            epoch,
            engine,
            program,
        })
    }
}

impl<'a> CloseMandate<'a> {
    pub const DISCRIMINATOR: &'a u8 = &22;

    pub fn process(&mut self) -> ProgramResult {
        // Only a mandate whose terms can never run again
        let mandate = Mandate::load(self.mandate)?;
        let expired = Clock::get()?.unix_timestamp >= mandate.not_after();
        let superseded = mandate_epoch(mandate, self.epoch)? != mandate.epoch();
        if !expired && !superseded {
            return Err(MandateError::NotClosable.into());
        }
        close(self.mandate, self.payer)?;

        // Log the CloseMandate Event
        emit(
            self.engine,
            self.program,
            *Self::DISCRIMINATOR,
            &[self.mandate.key()],
        )
    }
}
