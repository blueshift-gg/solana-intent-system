use crate::events::emit;
use crate::helpers::{balance, check_top_level, find, token_owner, transfer};
use crate::session;
use crate::state::{load_mandate, mandate_epoch};
use mandate_core::terms::{Refill, Terms};
use mandate_core::{constants::*, errors::MandateError};
use pinocchio::log::sol_log;
use pinocchio::sysvars::{clock::Clock, Sysvar};
use pinocchio::{account_info::AccountInfo, program_error::ProgramError, ProgramResult};

/// Bytes per pull: `from: u8, to: u8, amount: u64`.
const PULL_LEN: usize = 10;

/// # Open
///
/// Execute one mandate: pull what the executor asks, within every take.
/// A mandate with requirements is an exchange: it joins the transaction's
/// session, and `Close` checks what it requires. A mandate without is a
/// payment: this instruction is all of it, at any stack height.
///
/// > Check flags, epoch, window and executor
/// > Exchange: join the session and snapshot every required target
/// > Pull from the takes, as the engine delegate, only to the destinations they allow
/// > Exchange: add this mandate's requirements to the session
/// > Record what each take consumed
///
/// Accounts:
///
/// 1. executor:        [signer]
/// 2. session:         [mut]           exchange: PDA [SESSION_SEED, executor]; payment: unused
/// 3. payer:           [signer, mut]   exchange: funds the session rent if new; payment: unused
/// 4. mandate:         [mut]
/// 5. epoch:                           the mandate's Epoch PDA, possibly absent
/// 6. instructions:                    the instructions sysvar
/// 7. system_program:  [executable]
/// 8. engine:                          SPL delegate and event signer
/// 9. program:         [executable]    this program, for the event CPI
/// 10. accounts…:      [mut?]          token accounts, mints, token programs
///
/// Parameters:
/// 1. pulls: u8 + [(from: u8, to: u8, amount: u64)],   // indices into `accounts`
///
/// Account Checks:
/// - Executor: signer
/// - Mandate: writable, loaded in process
/// - Epoch: the account the mandate recorded, checked when read in process
/// - Session: loaded or created at its PDA in process
/// - Instructions: checked by the sysvar loader
/// - Payer, SystemProgram, Engine, Program: no need to check since the CPIs fail otherwise
///
/// Instruction Checks:
/// - Pulls: exact length
/// - Exchange: top-level only; payment: no session in the transaction
///
/// Event Data:
/// - discriminator: u8, (255u8, 20u8)
/// - mandate: Pubkey,
/// - authority: Pubkey,
/// - executor: Pubkey,
pub struct OpenAccounts<'a> {
    pub executor: &'a AccountInfo,
    pub session: &'a AccountInfo,
    pub payer: &'a AccountInfo,
    pub mandate: &'a AccountInfo,
    pub epoch: &'a AccountInfo,
    pub instructions: &'a AccountInfo,
    pub engine: &'a AccountInfo,
    pub program: &'a AccountInfo,
    pub accounts: &'a [AccountInfo],
}

impl<'a> TryFrom<&'a [AccountInfo]> for OpenAccounts<'a> {
    type Error = ProgramError;

    fn try_from(accounts: &'a [AccountInfo]) -> Result<Self, Self::Error> {
        let [executor, session, payer, mandate, epoch, instructions, _system_program, engine, program, accounts @ ..] =
            accounts
        else {
            return Err(ProgramError::NotEnoughAccountKeys);
        };

        // Account Checks
        if !executor.is_signer() {
            return Err(MandateError::NotSigner.into());
        }
        if !mandate.is_writable() {
            return Err(MandateError::NotMutable.into());
        }

        Ok(Self {
            executor,
            session,
            payer,
            mandate,
            epoch,
            instructions,
            engine,
            program,
            accounts,
        })
    }
}

pub struct OpenInstructionData<'a> {
    pub pulls: &'a [u8],
}

impl<'a> TryFrom<&'a [u8]> for OpenInstructionData<'a> {
    type Error = ProgramError;

    fn try_from(data: &'a [u8]) -> Result<Self, Self::Error> {
        let malformed = ProgramError::InvalidInstructionData;
        let (count, pulls) = data.split_first().ok_or(malformed.clone())?;

        // Instruction Checks
        if pulls.len() != *count as usize * PULL_LEN {
            return Err(malformed);
        }

        Ok(Self { pulls })
    }
}

pub struct Open<'a> {
    pub accounts: OpenAccounts<'a>,
    pub instruction_data: OpenInstructionData<'a>,
}

impl<'a> TryFrom<(&'a [u8], &'a [AccountInfo])> for Open<'a> {
    type Error = ProgramError;

    fn try_from((data, accounts): (&'a [u8], &'a [AccountInfo])) -> Result<Self, Self::Error> {
        sol_log("Open");

        Ok(Self {
            accounts: OpenAccounts::try_from(accounts)?,
            instruction_data: OpenInstructionData::try_from(data)?,
        })
    }
}

impl<'a> Open<'a> {
    pub const DISCRIMINATOR: &'a u8 = &20;

    pub fn process(&mut self) -> ProgramResult {
        let a = &self.accounts;
        let now = Clock::get()?.unix_timestamp;

        // Only a live mandate of the authority's current epoch
        let (mandate, bytes) = load_mandate(a.mandate)?;
        if mandate.flags() & REVOKED != 0 || mandate_epoch(mandate, a.epoch)? != mandate.epoch() {
            return Err(MandateError::Revoked.into());
        }
        if mandate.flags() & DONE != 0 {
            return Err(MandateError::AlreadyUsed.into());
        }

        // Only inside the window, and only by the named executor
        let terms = Terms::decode(bytes)?;
        if now < terms.not_before {
            return Err(MandateError::NotYetValid.into());
        }
        if terms.not_after.is_some_and(|t| now >= t) {
            return Err(MandateError::Expired.into());
        }
        if terms.executor.is_some_and(|e| e.ne(a.executor.key())) {
            return Err(MandateError::InvalidExecutor.into());
        }
        if terms.once {
            mandate.set_flags(mandate.flags() | DONE);
        }

        // An exchange joins the session and snapshots what it requires before
        // anything moves; a payment must stay out of any session
        let mut session = match terms.requires.is_empty() {
            true => {
                session::check_no_session(a.instructions)?;
                None
            }
            false => {
                check_top_level()?;
                let session = session::begin(a.executor, a.payer, a.session, a.instructions)?;
                for x in terms.requires.iter() {
                    session::entry(session, a.accounts, x.target, x.mint, x.owner)?;
                }
                Some(session)
            }
        };

        // What each take has consumed so far
        let mut spent = [0u64; MAX_ASSERTS];
        for (k, take) in terms.takes.iter().enumerate() {
            spent[k] = mandate.ledger.spent(k, take.max, take.refill, now);
        }

        // Pull what the executor asks, within every take on the source
        let mut taken = [0u64; MAX_ASSERTS];
        for pull in self.instruction_data.pulls.chunks_exact(PULL_LEN) {
            let from = a
                .accounts
                .get(pull[0] as usize)
                .ok_or(MandateError::MissingAccount)?;
            let to = a
                .accounts
                .get(pull[1] as usize)
                .ok_or(MandateError::MissingAccount)?;
            let amount = u64::from_le_bytes(pull[2..].try_into().unwrap());

            let mut mint = None;
            for (k, take) in terms.takes.iter().enumerate() {
                if take.from.ne(from.key()) {
                    continue;
                }
                // The engine is the delegate of many wallets: only the authority's own account
                balance(from, take.mint, terms.authority)?;
                if !take.to.is_empty() && !take.to.contains(to.key()) {
                    return Err(MandateError::InvalidPull.into());
                }
                taken[k] = taken[k].checked_add(amount).ok_or(MandateError::Overflow)?;
                let total = spent[k].checked_add(taken[k]);
                if total.ok_or(MandateError::Overflow)? > take.max {
                    return Err(MandateError::BudgetExceeded.into());
                }
                mint = Some(take.mint);
            }
            let mint = mint.ok_or(MandateError::InvalidPull)?;

            if let Some(session) = session.as_deref_mut() {
                // A later mandate may require what this pull pays in, so the
                // destination is snapshotted before the transfer
                session::entry(session, a.accounts, to.key(), mint, token_owner(to)?)?;
                let source =
                    session::entry(session, a.accounts, from.key(), mint, terms.authority)?;
                source.set_required(source.required() - amount as i128);
                source.set_kind(BOUND);
            }
            transfer(from, find(a.accounts, mint)?, to, a.engine, amount)?;
        }

        // Record what Close must see: every requirement at its current bound
        if let Some(session) = session {
            for x in terms.requires.iter() {
                let bound = x.bound.at(now, &taken)? as i128;
                let entry = session::entry(session, a.accounts, x.target, x.mint, x.owner)?;
                let required = entry.required().checked_add(bound);
                entry.set_required(required.ok_or(MandateError::Overflow)?);
                entry.set_kind(BOUND);
            }
        }

        // Settle the ledger; the mandate is done once no account can be pulled
        // again. Only a pull writes it: each write rounds a refill down by under
        // one unit, and an Open that takes nothing must not cost the executor that
        if taken.iter().any(|t| *t != 0) {
            for k in 0..terms.takes.len() {
                mandate.ledger.set_consumed(k, spent[k] + taken[k]);
            }
            mandate.ledger.set_rolled(now);
            let spent_for_good = |from| {
                let mut takes = terms.takes.iter().enumerate();
                takes.any(|(k, t)| {
                    t.from.eq(from) && t.refill == Refill::Never && spent[k] + taken[k] == t.max
                })
            };
            if terms.takes.iter().all(|t| spent_for_good(t.from)) {
                mandate.set_flags(mandate.flags() | DONE);
            }
        }

        // Log the Open Event
        emit(
            a.engine,
            a.program,
            *Self::DISCRIMINATOR,
            &[a.mandate.key(), terms.authority, a.executor.key()],
        )
    }
}
