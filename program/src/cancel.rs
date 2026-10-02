use crate::events::emit;
use crate::state::nonces_for;
use mandate_core::errors::MandateError;
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # Cancel
///
/// Use up the nonce of a signed intent, so it can never be filled.
///
/// > Mark the nonce used, creating its page if needed
///
/// Accounts:
///
/// 1. authority:       [signer]
/// 2. payer:           [signer, mut]   funds the page of nonces if it is new
/// 3. nonces:          [mut]           PDA [NONCES_SEED, authority, expiry day]
/// 4. system_program:  [executable]
/// 5. engine:                          event signer
/// 6. program:         [executable]    this program, for the event CPI
///
/// Parameters:
/// 1. not_after: i64,      // the intent's expiry
/// 2. salt: u64,           // the intent's salt
///
/// Account Checks:
/// - Authority: signer; the page is derived from it, so it can only cancel its own
/// - Nonces: writable; the PDA, or created there, checked in process
/// - Payer, SystemProgram, Engine, Program: no need to check since the CPIs fail otherwise
///
/// Event Data:
/// - discriminator: u8, (255u8, 11u8)
/// - authority: Pubkey,
/// - salt: u64,
pub struct Cancel<'a> {
    pub authority: &'a AccountInfo,
    pub payer: &'a AccountInfo,
    pub nonces: &'a AccountInfo,
    pub engine: &'a AccountInfo,
    pub program: &'a AccountInfo,
    pub not_after: i64,
    pub salt: u64,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Cancel<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("Cancel");

        let [authority, payer, nonces, _system_program, engine, program] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };
        let malformed = ProgramError::InvalidInstructionData;
        let (not_after, salt) = data.split_first_chunk::<8>().ok_or(malformed.clone())?;

        // Account Checks
        if !authority.is_signer() {
            return Err(MandateError::NotSigner.into());
        }
        if !nonces.is_writable() {
            return Err(MandateError::NotMutable.into());
        }

        Ok(Self {
            authority,
            payer,
            nonces,
            engine,
            program,
            not_after: i64::from_le_bytes(*not_after),
            salt: u64::from_le_bytes(salt.try_into().map_err(|_| malformed)?),
        })
    }
}

impl<'a> Cancel<'a> {
    pub const DISCRIMINATOR: &'a u8 = &11;

    pub fn process(&mut self) -> ProgramResult {
        let authority = self.authority.key();

        // Cancelling what was used or cancelled already changes nothing
        nonces_for(self.payer, self.nonces, authority, self.not_after)?.take(self.salt);

        // Log the Cancel Event
        emit(
            self.engine,
            self.program,
            *Self::DISCRIMINATOR,
            &[authority, &self.salt.to_le_bytes()],
        )
    }
}
