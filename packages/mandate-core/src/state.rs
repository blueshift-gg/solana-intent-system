//! Account layouts. Every struct has alignment 1: fields are little-endian
//! byte arrays decoded on access, so a struct overlays raw account bytes.
//! Every account starts with a tag naming its type, so one account can never
//! be read as another. Callers validate owner, length and tag before taking views.

use crate::{constants::*, terms::Refill};
use pinocchio::pubkey::Pubkey;

macro_rules! field {
    ($get:ident, $set:ident, $t:ty) => {
        #[inline(always)]
        pub fn $get(&self) -> $t {
            <$t>::from_le_bytes(self.$get)
        }
        #[inline(always)]
        pub fn $set(&mut self, v: $t) {
            self.$get = v.to_le_bytes();
        }
    };
}

macro_rules! account {
    ($name:ident) => {
        impl $name {
            /// # Safety
            /// `bytes` holds at least `size_of::<Self>()` bytes.
            #[inline(always)]
            pub unsafe fn from_bytes_unchecked(bytes: &[u8]) -> &Self {
                &*(bytes.as_ptr() as *const Self)
            }

            /// # Safety
            /// `bytes` holds at least `size_of::<Self>()` bytes.
            #[inline(always)]
            pub unsafe fn from_bytes_unchecked_mut(bytes: &mut [u8]) -> &mut Self {
                &mut *(bytes.as_mut_ptr() as *mut Self)
            }
        }
    };
}

/// What each take of a mandate has consumed.
#[repr(C)]
pub struct Ledger {
    /// When `consumed` was last written; refills count from here.
    rolled: [u8; 8],
    consumed: [[u8; 8]; MAX_ASSERTS],
}

impl Ledger {
    field!(rolled, set_rolled, i64);

    pub fn set_consumed(&mut self, k: usize, v: u64) {
        self.consumed[k] = v.to_le_bytes();
    }

    /// What take `k` of `max` has consumed at `now`.
    pub fn spent(&self, k: usize, max: u64, refill: Refill, now: i64) -> u64 {
        let consumed = u64::from_le_bytes(self.consumed[k]);
        match refill {
            Refill::Never => consumed,
            Refill::Over { period } => {
                let elapsed = now.saturating_sub(self.rolled()).clamp(0, period as i64) as u128;
                let back = (max as u128 * elapsed / period as u128) as u64;
                consumed.saturating_sub(back)
            }
            Refill::EachUse => 0,
        }
    }
}

/// A mandate: this header, then the canonical terms. A mandate revoked
/// before it was ever created is the header alone. Either way the account is
/// the mandate's tombstone: it stays until the terms can never run again.
#[repr(C)]
pub struct Mandate {
    tag: [u8; 1],
    flags: [u8; 1],
    pub ledger: Ledger,
    /// `i64::MAX` when the terms never expire.
    not_after: [u8; 8],
    /// The authority's epoch at creation; a later one revokes the mandate.
    epoch: [u8; 4],
    pub authority: Pubkey,
    /// Paid the rent; refunded by `CloseMandate`.
    pub payer: Pubkey,
    /// The authority's Epoch PDA, derived once at creation.
    pub epoch_account: Pubkey,
    terms_len: [u8; 2],
}

account!(Mandate);

impl Mandate {
    field!(tag, set_tag, u8);
    field!(flags, set_flags, u8);
    field!(not_after, set_not_after, i64);
    field!(epoch, set_epoch, u32);
    field!(terms_len, set_terms_len, u16);
}

#[repr(C)]
pub struct Epoch {
    tag: [u8; 1],
    pub authority: Pubkey,
    epoch: [u8; 4],
}

account!(Epoch);

impl Epoch {
    field!(tag, set_tag, u8);
    field!(epoch, set_epoch, u32);
}

/// A token account the session watches: its balance at first sight, and the
/// summed change `Close` requires.
#[repr(C)]
pub struct Entry {
    pub target: Pubkey,
    pub mint: Pubkey,
    pub owner: Pubkey,
    kind: [u8; 1],
    snapshot: [u8; 16],
    required: [u8; 16],
}

impl Entry {
    field!(kind, set_kind, u8);
    field!(snapshot, set_snapshot, i128);
    field!(required, set_required, i128);
}

/// The per-executor record between the first `Open` and `Close` of a transaction.
#[repr(C)]
pub struct Session {
    tag: [u8; 1],
    active: [u8; 1],
    count: [u8; 1],
    pub executor: Pubkey,
    pub entries: [Entry; MAX_ENTRIES],
}

account!(Session);

impl Session {
    field!(tag, set_tag, u8);
    field!(active, set_active, u8);
    field!(count, set_count, u8);

    pub fn entries(&mut self) -> &mut [Entry] {
        let count = self.count() as usize;
        &mut self.entries[..count]
    }
}

const _: () = {
    assert!(core::mem::size_of::<Ledger>() == LEDGER_LEN);
    assert!(core::mem::size_of::<Mandate>() == MANDATE_LEN);
    assert!(core::mem::size_of::<Epoch>() == EPOCH_LEN);
    assert!(core::mem::size_of::<Entry>() == ENTRY_LEN);
    assert!(core::mem::size_of::<Session>() == SESSION_LEN);
};
