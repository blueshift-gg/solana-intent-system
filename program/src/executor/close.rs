use crate::events::emit;
use crate::helpers::{balance, check_top_level, find};
use crate::state::{Load, Session};
use mandate_core::{constants::*, errors::MandateError};
use pinocchio::log::sol_log;
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// # Close
///
/// Check every outcome the session's mandates require, then clear it.
///
/// > Every bounded entry: the target moved by at least the summed bound since its snapshot
/// > Deactivate the session
///
/// Accounts:
///
/// 1. executor:        [signer]
/// 2. session:         [mut]           PDA [SESSION_SEED, executor]
/// 3. engine:                          event signer
/// 4. program:         [executable]    this program, for the event CPI
/// 5. targets…:                        every target an entry names
///
/// Account Checks:
/// - Executor: signer, the session's executor
/// - Session: writable, loaded, active
/// - Engine, Program: no need to check since the event CPI fails otherwise
///
/// Instruction Checks:
/// - Top-level only
///
/// Event Data:
/// - discriminator: u8, (255u8, 21u8)
/// - executor: Pubkey,
/// - entries: u8,
pub struct Close<'a> {
    pub executor: &'a AccountInfo,
    pub session: &'a mut Session,
    pub engine: &'a AccountInfo,
    pub program: &'a AccountInfo,
    pub targets: &'a [AccountInfo],
}

impl<'a> TryFrom<&'a [AccountInfo]> for Close<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        sol_log("Close");
        check_top_level()?;

        let [executor, session, engine, program, targets @ ..] = accounts else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !executor.is_signer() {
            return Err(MandateError::NotSigner.into());
        }
        if !session.is_writable() {
            return Err(MandateError::NotMutable.into());
        }
        let session = Session::load(session)?;
        if session.executor.ne(executor.key()) || session.active() == 0 {
            return Err(MandateError::InvalidSession.into());
        }

        Ok(Self {
            executor,
            session,
            engine,
            program,
            targets,
        })
    }
}

impl<'a> Close<'a> {
    pub const DISCRIMINATOR: &'a u8 = &21;

    pub fn process(&mut self) -> ProgramResult {
        // Every outcome must hold
        for entry in self
            .session
            .entries()
            .iter()
            .filter(|e| e.kind() != UNBOUND)
        {
            let target = find(self.targets, &entry.target)?;
            let moved = balance(target, &entry.mint, &entry.owner)? - entry.snapshot();
            if moved < entry.required() {
                return Err(MandateError::OutcomeNotMet.into());
            }
        }

        // End the session
        let entries = self.session.count();
        self.session.set_active(0);
        self.session.set_count(0);

        // Log the Close Event
        emit(
            self.engine,
            self.program,
            *Self::DISCRIMINATOR,
            &[self.executor.key(), &[entries]],
        )
    }
}
