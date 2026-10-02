//! The mandate account view. Every `unsafe` needed to overlay the layout on
//! account bytes lives here; handlers only see checked views.

pub use mandate_core::state::Mandate;
use mandate_core::{constants::*, errors::MandateError};
use pinocchio::{account_info::AccountInfo, program_error::ProgramError};

/// A mandate's header and the canonical terms after it, after checking the
/// account's owner, length and tag. The program never holds a checked borrow,
/// so callers must not alias the header.
#[allow(clippy::mut_from_ref)]
pub fn load(account: &AccountInfo) -> Result<(&mut Mandate, &[u8]), ProgramError> {
    if !account.is_owned_by(&crate::ID) {
        return Err(MandateError::InvalidAccountOwner.into());
    }
    if account.data_len() < MANDATE_LEN {
        return Err(MandateError::InvalidAccountLength.into());
    }
    // SAFETY: length checked above; all fields have alignment 1.
    let header = unsafe { Mandate::from_bytes_unchecked_mut(account.borrow_mut_data_unchecked()) };
    if header.tag() != MANDATE_TAG {
        return Err(MandateError::InvalidTag.into());
    }
    let end = MANDATE_LEN + header.terms_len() as usize;
    // SAFETY: the terms lie after the header, so this view never overlaps `header`.
    let data = unsafe { account.borrow_data_unchecked() };
    let terms = data
        .get(MANDATE_LEN..end)
        .ok_or(MandateError::InvalidAccountLength)?;
    Ok((header, terms))
}
