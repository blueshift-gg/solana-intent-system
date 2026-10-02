//! Account layouts. Every struct has alignment 1: fields are little-endian
//! byte arrays decoded on access, so a struct overlays raw account bytes.
//! Every account starts with a tag naming its type, so one account can never
//! be read as another. Callers validate owner, length and tag before taking views.

use crate::{constants::*, terms::Per};
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

/// What each limit of a policy has consumed.
#[repr(C)]
pub struct Ledger {
    /// When `consumed` was last written.
    rolled: [u8; 8],
    consumed: [[u8; 8]; MAX_LIMITS],
}

impl Ledger {
    field!(rolled, set_rolled, i64);

    pub fn set_consumed(&mut self, k: usize, v: u64) {
        self.consumed[k] = v.to_le_bytes();
    }

    /// What limit `k` has consumed at `now`. A periodic limit starts each
    /// window, counted from `start`, with nothing consumed.
    pub fn spent(&self, k: usize, per: Per, start: i64, now: i64) -> u64 {
        let consumed = u64::from_le_bytes(self.consumed[k]);
        match per {
            Per::Total => consumed,
            Per::Every(seconds) => {
                let window = |t: i64| (t - start).div_euclid(seconds as i64);
                match window(now) == window(self.rolled()) {
                    true => consumed,
                    false => 0,
                }
            }
            Per::Use => 0,
        }
    }
}

/// A policy: this header, then the canonical terms.
#[repr(C)]
pub struct Policy {
    tag: [u8; 1],
    pub ledger: Ledger,
    /// Paid the rent; refunded by `Close`.
    pub payer: Pubkey,
    terms_len: [u8; 2],
}

account!(Policy);

impl Policy {
    field!(tag, set_tag, u8);
    field!(terms_len, set_terms_len, u16);
}

/// The nonces an authority's signed intents have used, for intents that
/// expire on one day. One bit each, so intents run in any order.
#[repr(C)]
pub struct Nonces {
    tag: [u8; 1],
    /// Paid the rent; refunded by `Close` once the day is over.
    pub payer: Pubkey,
    day: [u8; 8],
    bits: [u8; NONCE_BITS / 8],
}

account!(Nonces);

impl Nonces {
    field!(tag, set_tag, u8);
    field!(day, set_day, i64);

    /// Mark `nonce` used; false if it already was.
    pub fn take(&mut self, nonce: u64) -> bool {
        let bit = (nonce % NONCE_BITS as u64) as usize;
        let (byte, mask) = (&mut self.bits[bit / 8], 1 << (bit % 8));
        let fresh = *byte & mask == 0;
        *byte |= mask;
        fresh
    }
}

const _: () = {
    assert!(core::mem::size_of::<Ledger>() == LEDGER_LEN);
    assert!(core::mem::size_of::<Policy>() == POLICY_LEN);
    assert!(core::mem::size_of::<Nonces>() == NONCES_LEN);
};
