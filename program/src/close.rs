use crate::events::emit;
use crate::helpers::close;
use crate::state::{mandate, nonces};
use mandate_core::terms::Terms;
use mandate_core::{constants::*, errors::MandateError};
use pinocchio::log::sol_log;
use pinocchio::sysvars::{clock::Clock, Sysvar};
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # Close
///
/// Close a mandate or a page of nonces, and return its rent to whoever paid it.
///
/// A mandate: its authority or its spender may at any time, and anyone once
/// it has expired. Only a transaction can create one, so it is gone for good.
///
/// A page of nonces: anyone, once its day is over. Every intent it guarded
/// has expired by then, so no signature can run again.
///
/// > Close the account into its payer
///
/// Accounts:
///
/// 1. closer:          [signer]
/// 2. account:         [mut]           a Mandate or a page of Nonces
/// 3. payer:           [mut]           the recorded payer, receives the rent
/// 4. engine:                          event signer
/// 5. program:         [executable]    this program, for the event CPI
///
/// Account Checks:
/// - Closer: signer; allowed to close, checked in process
/// - Account: writable, loaded in process
/// - Payer: writable, the recorded payer, checked in process
/// - Engine, Program: no need to check since the event CPI fails otherwise
///
/// Event Data:
/// - discriminator: u8, (255u8, 2u8)
/// - account: Pubkey,
/// - closer: Pubkey,
pub struct Close<'a> {
    pub closer: &'a AccountInfo,
    pub account: &'a AccountInfo,
    pub payer: &'a AccountInfo,
    pub engine: &'a AccountInfo,
    pub program: &'a AccountInfo,
}

impl<'a> TryFrom<&'a [AccountInfo]> for Close<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        sol_log("Close");

        let [closer, account, payer, engine, program] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !closer.is_signer() {
            return Err(MandateError::NotSigner.into());
        }
        if !account.is_writable() || !payer.is_writable() {
            return Err(MandateError::NotMutable.into());
        }

        Ok(Self {
            closer,
            account,
            payer,
            engine,
            program,
        })
    }
}

impl<'a> Close<'a> {
    pub const DISCRIMINATOR: &'a u8 = &2;

    pub fn process(&mut self) -> ProgramResult {
        let now = Clock::get()?.unix_timestamp;
        let closer = self.closer.key();

        // Who paid, and whether this closer may close it now
        let (payer, allowed) = match nonces(self.account) {
            Ok(page) => (page.payer, now >= (page.day() + 1) * NONCE_DAY),
            Err(_) => {
                let (mandate, bytes) = mandate(self.account)?;
                let terms = Terms::decode(bytes)?;
                let party =
                    terms.authority.eq(closer) || terms.spender.is_some_and(|s| s.eq(closer));
                (
                    mandate.payer,
                    party || terms.not_after.is_some_and(|t| now >= t),
                )
            }
        };
        if !allowed {
            return Err(MandateError::NotClosable.into());
        }
        if payer.ne(self.payer.key()) {
            return Err(MandateError::InvalidPayer.into());
        }
        close(self.account, self.payer)?;

        // Log the Close Event
        emit(
            self.engine,
            self.program,
            *Self::DISCRIMINATOR,
            &[self.account.key(), closer],
        )
    }
}
