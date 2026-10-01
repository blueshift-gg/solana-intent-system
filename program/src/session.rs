//! The session: one per transaction, between the first `Open` of an exchange
//! and the single `Close`. An account is snapshotted the first time a mandate
//! requires it or a pull touches it, whichever comes first, and bounds on it
//! are summed: one deposit can never satisfy two mandates, and one mandate's
//! pull still counts toward another's requirement, so mandates net. Payments
//! never enter it.

use crate::helpers::{balance, find};
use crate::state::{Load, Session};
use crate::{Close, Open};
use mandate_core::{constants::*, errors::MandateError, state::Entry};
use pinocchio::{
    account_info::AccountInfo,
    program_error::ProgramError,
    pubkey::Pubkey,
    sysvars::instructions::{Instructions, IntrospectedInstruction},
};

/// The executor's session, created on first use with rent from `payer`. The
/// first `Open` of a transaction activates it after checking the transaction's shape.
#[allow(clippy::mut_from_ref)]
pub fn begin<'a>(
    executor: &AccountInfo,
    payer: &AccountInfo,
    account: &'a AccountInfo,
    instructions: &AccountInfo,
) -> Result<&'a mut Session, ProgramError> {
    let session = Session::load_or_create(payer, account, &[SESSION_SEED, executor.key()], |s| {
        s.executor = *executor.key();
    })?;
    if session.executor.ne(executor.key()) {
        return Err(MandateError::InvalidSession.into());
    }
    if session.active() == 0 {
        check_transaction(account, instructions)?;
        session.set_active(1);
        session.set_count(0);
    }
    Ok(session)
}

/// Every engine `Open`/`Close` in the transaction names this session, exactly
/// one `Close` follows the current instruction, and no `Open` follows it.
fn check_transaction(
    session: &AccountInfo,
    instructions: &AccountInfo,
) -> Result<(), ProgramError> {
    let instructions = Instructions::try_from(instructions)?;
    let current = instructions.load_current_index() as usize;
    let mut closes = 0;
    for index in 0.. {
        let Ok(ix) = instructions.load_instruction_at(index) else {
            break;
        };
        let Some(kind) = engine_kind(&ix) else {
            continue;
        };
        if ix.get_account_meta_at(1)?.key.ne(session.key()) {
            return Err(MandateError::InvalidSession.into());
        }
        match kind {
            Close::DISCRIMINATOR if index > current => closes += 1,
            Open::DISCRIMINATOR if closes > 0 => return Err(MandateError::InvalidSession.into()),
            _ => {}
        }
    }
    if closes != 1 {
        return Err(MandateError::InvalidSession.into());
    }
    Ok(())
}

/// A payment runs outside any session: no engine `Close` may follow it in the
/// transaction. Inside one, what it pays into an account would count toward
/// another mandate's requirement on that account without being claimed.
pub fn check_no_session(instructions: &AccountInfo) -> Result<(), ProgramError> {
    let instructions = Instructions::try_from(instructions)?;
    for index in instructions.load_current_index() as usize + 1.. {
        let Ok(ix) = instructions.load_instruction_at(index) else {
            break;
        };
        if engine_kind(&ix) == Some(Close::DISCRIMINATOR) {
            return Err(MandateError::PaymentInSession.into());
        }
    }
    Ok(())
}

/// The discriminator of an engine `Open` or `Close`, if `ix` is one.
fn engine_kind<'a>(ix: &'a IntrospectedInstruction) -> Option<&'a u8> {
    if ix.get_program_id().ne(&crate::ID) {
        return None;
    }
    ix.get_instruction_data()
        .first()
        .filter(|d| *d == Open::DISCRIMINATOR || *d == Close::DISCRIMINATOR)
}

/// The entry for `target`, snapshotting it as `UNBOUND` if this session has
/// not seen it yet. A second mandate must name the same mint and owner.
pub fn entry<'s>(
    session: &'s mut Session,
    accounts: &[AccountInfo],
    target: &Pubkey,
    mint: &Pubkey,
    owner: &Pubkey,
) -> Result<&'s mut Entry, MandateError> {
    let seen = session.entries().iter().position(|e| e.target.eq(target));
    let index = match seen {
        Some(index) => index,
        None => {
            let snapshot = balance(find(accounts, target)?, mint, owner)?;
            let index = session.count() as usize;
            let entry = session
                .entries
                .get_mut(index)
                .ok_or(MandateError::SessionFull)?;
            entry.target = *target;
            entry.mint = *mint;
            entry.owner = *owner;
            entry.set_kind(UNBOUND);
            entry.set_snapshot(snapshot);
            entry.set_required(0);
            session.set_count(index as u8 + 1);
            index
        }
    };
    let entry = &mut session.entries()[index];
    if entry.mint.ne(mint) || entry.owner.ne(owner) {
        return Err(MandateError::InvalidTarget);
    }
    Ok(entry)
}
