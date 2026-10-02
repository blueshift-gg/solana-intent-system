//! Account views. Every `unsafe` needed to overlay a layout on account bytes
//! lives here; handlers only see checked views. The program never holds a
//! checked borrow, so callers must not alias a view.

use crate::helpers::{check_pda, create_pda};
pub use mandate_core::state::{Nonces, Policy};
use mandate_core::{constants::*, errors::MandateError};
use pinocchio::{account_info::AccountInfo, program_error::ProgramError};

/// The account's bytes, after checking its owner, length and tag.
#[allow(clippy::mut_from_ref)]
fn bytes(account: &AccountInfo, len: usize, tag: u8) -> Result<&mut [u8], ProgramError> {
    if !account.is_owned_by(&crate::ID) {
        return Err(MandateError::InvalidAccountOwner.into());
    }
    if account.data_len() < len {
        return Err(MandateError::InvalidAccountLength.into());
    }
    // SAFETY: see the module doc; nothing else borrows the data.
    let data = unsafe { account.borrow_mut_data_unchecked() };
    if data[0] != tag {
        return Err(MandateError::InvalidTag.into());
    }
    Ok(data)
}

/// A mandate's header and the canonical terms after it.
#[allow(clippy::mut_from_ref)]
pub fn policy(account: &AccountInfo) -> Result<(&mut Policy, &[u8]), ProgramError> {
    let (header, rest) = bytes(account, POLICY_LEN, POLICY_TAG)?.split_at_mut(POLICY_LEN);
    // SAFETY: `header` holds exactly the layout; all fields have alignment 1.
    let header = unsafe { Policy::from_bytes_unchecked_mut(header) };
    let terms = rest
        .get(..header.terms_len() as usize)
        .ok_or(MandateError::InvalidAccountLength)?;
    Ok((header, terms))
}

/// An existing page of nonces.
#[allow(clippy::mut_from_ref)]
pub fn nonces(account: &AccountInfo) -> Result<&mut Nonces, ProgramError> {
    // SAFETY: length checked by `bytes`; all fields have alignment 1.
    Ok(unsafe { Nonces::from_bytes_unchecked_mut(bytes(account, NONCES_LEN, NONCES_TAG)?) })
}

/// The page of nonces for intents of `authority` that expire at `not_after`,
/// created with rent from `payer` if this is the first of its day.
#[allow(clippy::mut_from_ref)]
pub fn nonces_for<'a>(
    payer: &AccountInfo,
    account: &'a AccountInfo,
    authority: &[u8; 32],
    not_after: i64,
) -> Result<&'a mut Nonces, ProgramError> {
    // A page records nothing of its authority: its address is the proof
    let day = not_after.div_euclid(NONCE_DAY);
    let seeds: [&[u8]; 3] = [NONCES_SEED, authority, &day.to_le_bytes()];
    let bump = check_pda(account, &seeds)?;
    if account.is_owned_by(&crate::ID) {
        return nonces(account);
    }
    create_pda(payer, account, NONCES_LEN, &seeds, bump)?;
    // SAFETY: the account was just created with `NONCES_LEN` zeroed bytes.
    let page = unsafe { Nonces::from_bytes_unchecked_mut(account.borrow_mut_data_unchecked()) };
    page.set_tag(NONCES_TAG);
    page.payer = *payer.key();
    page.set_day(day);
    Ok(page)
}
