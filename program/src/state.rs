//! Program-owned account views. Every `unsafe` needed to overlay a layout on
//! account bytes lives here; handlers only see checked views.

use crate::helpers::{check_pda, create_pda};
pub use mandate_core::state::{Epoch, Mandate, Session};
use mandate_core::{constants::*, errors::MandateError};
use pinocchio::{account_info::AccountInfo, program_error::ProgramError};

pub trait Load: Sized {
    const LEN: usize;
    const TAG: u8;

    fn tag(&self) -> u8;

    fn set_tag(&mut self, tag: u8);

    /// # Safety
    /// `bytes` holds at least `Self::LEN` bytes.
    unsafe fn view(bytes: &mut [u8]) -> &mut Self;

    /// View the account after checking owner, length and tag. The program
    /// never holds a checked borrow, so callers must not alias a view.
    #[allow(clippy::mut_from_ref)]
    fn load(account: &AccountInfo) -> Result<&mut Self, ProgramError> {
        let this = Self::load_raw(account)?;
        if this.tag() != Self::TAG {
            return Err(MandateError::InvalidTag.into());
        }
        Ok(this)
    }

    #[allow(clippy::mut_from_ref)]
    fn load_raw(account: &AccountInfo) -> Result<&mut Self, ProgramError> {
        if !account.is_owned_by(&crate::ID) {
            return Err(MandateError::InvalidAccountOwner.into());
        }
        if account.data_len() < Self::LEN {
            return Err(MandateError::InvalidAccountLength.into());
        }
        // SAFETY: length checked above; all fields have alignment 1.
        Ok(unsafe { Self::view(account.borrow_mut_data_unchecked()) })
    }

    /// View the account, first creating it at the PDA for `seeds` (rent from
    /// `payer`) and running `init` if it does not exist yet.
    #[allow(clippy::mut_from_ref)]
    fn load_or_create<'a>(
        payer: &AccountInfo,
        account: &'a AccountInfo,
        seeds: &[&[u8]],
        init: impl FnOnce(&mut Self),
    ) -> Result<&'a mut Self, ProgramError> {
        if !account.is_owned_by(&crate::ID) {
            let bump = check_pda(account, seeds)?;
            create_pda(payer, account, Self::LEN, seeds, bump)?;
            let this = Self::load_raw(account)?;
            this.set_tag(Self::TAG);
            init(this);
        }
        Self::load(account)
    }
}

macro_rules! load {
    ($name:ident, $len:expr, $tag:expr) => {
        impl Load for $name {
            const LEN: usize = $len;
            const TAG: u8 = $tag;

            fn tag(&self) -> u8 {
                $name::tag(self)
            }

            fn set_tag(&mut self, tag: u8) {
                $name::set_tag(self, tag)
            }

            unsafe fn view(bytes: &mut [u8]) -> &mut Self {
                $name::from_bytes_unchecked_mut(bytes)
            }
        }
    };
}

load!(Mandate, MANDATE_LEN, MANDATE_TAG);
load!(Epoch, EPOCH_LEN, EPOCH_TAG);
load!(Session, SESSION_LEN, SESSION_TAG);

/// The epoch an Epoch account holds. The account is absent until the first
/// `BumpEpoch`, so callers check its address rather than its existence.
pub fn read_epoch(account: &AccountInfo) -> Result<u64, ProgramError> {
    match account.is_owned_by(&crate::ID) {
        true => Ok(Epoch::load(account)?.epoch()),
        false => Ok(0),
    }
}

/// The authority's current epoch, after deriving its Epoch PDA.
pub fn current_epoch(account: &AccountInfo, authority: &[u8; 32]) -> Result<u64, ProgramError> {
    check_pda(account, &[EPOCH_SEED, authority])?;
    read_epoch(account)
}

/// A mandate's current epoch. Its header holds the Epoch PDA, so nothing is derived.
pub fn mandate_epoch(mandate: &Mandate, account: &AccountInfo) -> Result<u64, ProgramError> {
    if account.key().ne(&mandate.epoch_account) {
        return Err(MandateError::InvalidSeeds.into());
    }
    read_epoch(account)
}

/// A mandate's header and the canonical terms after it.
#[allow(clippy::mut_from_ref)]
pub fn load_mandate(account: &AccountInfo) -> Result<(&mut Mandate, &[u8]), ProgramError> {
    let header = Mandate::load(account)?;
    let end = MANDATE_LEN + header.terms_len() as usize;
    // SAFETY: the terms lie after the header, so this view never overlaps `header`.
    let data = unsafe { account.borrow_data_unchecked() };
    let terms = data
        .get(MANDATE_LEN..end)
        .ok_or(MandateError::InvalidAccountLength)?;
    Ok((header, terms))
}
