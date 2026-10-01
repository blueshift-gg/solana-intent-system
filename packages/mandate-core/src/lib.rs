//! Terms, canonical encoding and text, account layouts and errors shared by the
//! Mandate program and its clients.
#![no_std]

pub mod constants;
pub mod errors;
pub mod render;
pub mod state;
pub mod terms;

use pinocchio::pubkey::Pubkey;

pub const ID: Pubkey = five8_const::decode_32_const("Mand89p7P6okjEKQx2SpwDX6mdb5zAcdpshRafFtv7A");

/// Anything bytes can be written to: a hasher on chain, a buffer off chain.
/// Encoding and rendering write through it, so neither needs to allocate.
pub trait Sink {
    fn put(&mut self, bytes: &[u8]);
}

#[cfg(feature = "std")]
extern crate std;

#[cfg(feature = "std")]
impl Sink for std::vec::Vec<u8> {
    fn put(&mut self, bytes: &[u8]) {
        self.extend_from_slice(bytes);
    }
}
