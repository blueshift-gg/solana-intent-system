use crate::events::emit;
use crate::helpers::close;
use crate::state::load;
use mandate_core::terms::Terms;
use mandate_core::{constants::*, errors::MandateError};
use pinocchio::log::sol_log;
use pinocchio::sysvars::{clock::Clock, Sysvar};
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # Close
///
/// End a mandate and return its rent to whoever paid it. The authority or the
/// spender may at any time; anyone may once it has expired.
///
/// A mandate that never expires closes at once: only a transaction can have
/// created it. One that expires may have a signature behind it, so before its
/// expiry it is only marked revoked and stays, or that signature could create
/// it again. Close it once more after the expiry to get the rent.
///
/// > Before the expiry of a mandate that expires: mark it revoked
/// > Otherwise: close the Mandate into its payer
///
/// Accounts:
///
/// 1. closer:          [signer]
/// 2. mandate:         [mut]
/// 3. payer:           [mut]           the recorded payer, receives the rent
/// 4. engine:                          event signer
/// 5. program:         [executable]    this program, for the event CPI
///
/// Account Checks:
/// - Closer: signer; the authority or the spender unless the mandate expired, checked in process
/// - Mandate: writable, loaded in process
/// - Payer: writable, the recorded payer, checked in process
/// - Engine, Program: no need to check since the event CPI fails otherwise
///
/// Event Data:
/// - discriminator: u8, (255u8, 2u8)
/// - mandate: Pubkey,
/// - closer: Pubkey,
pub struct Close<'a> {
    pub closer: &'a AccountInfo,
    pub mandate: &'a AccountInfo,
    pub payer: &'a AccountInfo,
    pub engine: &'a AccountInfo,
    pub program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for Close<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        sol_log("Close");

        let [closer, mandate, payer, engine, program] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !closer.is_signer() {
            return Err(MandateError::NotSigner.into());
        }
        if !mandate.is_writable() || !payer.is_writable() {
            return Err(MandateError::NotMutable.into());
        }

        Ok(Self {
            closer,
            mandate,
            payer,
            engine,
            program,
        })
    }
}

impl<'a> Close<'a> {
    pub const DISCRIMINATOR: &'a u8 = &2;

    pub fn process(&mut self) -> ProgramResult {
        let (mandate, bytes) = load(self.mandate)?;
        let terms = Terms::decode(bytes)?;
        let closer = self.closer.key();

        // The authority and the spender decide; after the expiry anyone may
        let expired = terms
            .not_after
            .is_some_and(|t| Clock::get().is_ok_and(|c| c.unix_timestamp >= t));
        let party = terms.authority.eq(closer) || terms.spender.is_some_and(|s| s.eq(closer));
        if !party && !expired {
            return Err(MandateError::NotClosable.into());
        }

        match terms.not_after.is_some() && !expired {
            // A signature for these terms may exist until they expire
            true => mandate.set_flags(REVOKED),
            false => {
                if mandate.payer.ne(self.payer.key()) {
                    return Err(MandateError::InvalidPayer.into());
                }
                close(self.mandate, self.payer)?;
            }
        }

        // Log the Close Event
        emit(
            self.engine,
            self.program,
            *Self::DISCRIMINATOR,
            &[self.mandate.key(), closer],
        )
    }
}
