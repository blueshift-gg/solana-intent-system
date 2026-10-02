use five8_const::decode_32_const;
use pinocchio::pubkey::Pubkey;

/// Terms format version.
pub const VERSION: u8 = 1;
/// The cluster this build accepts terms for. There is no genesis-hash
/// syscall, so the cluster is a compile-time constant.
pub const CLUSTER: u8 = if cfg!(feature = "localnet") {
    3
} else if cfg!(feature = "testnet") {
    2
} else if cfg!(feature = "devnet") {
    1
} else {
    0
};

/// Limits in one mandate.
pub const MAX_LIMITS: usize = 8;
/// 9999-12-31T23:59:59Z: the last instant the canonical text can render.
pub const MAX_TIME: i64 = 253_402_300_799;

pub const TOKEN_PROGRAM: Pubkey = decode_32_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const TOKEN_2022_PROGRAM: Pubkey =
    decode_32_const("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

/// Engine PDA: [ENGINE_SEED]. The SPL delegate of every enabled token account
/// and the signer of every event; the test suite re-derives it.
pub const ENGINE_SEED: &[u8] = b"engine";
pub const ENGINE: Pubkey = decode_32_const("6NpP2w9pBwSWBkQ7yNPYo5ruPY47goYB8DNsjBuKGyHp");
pub const ENGINE_BUMP: u8 = 254;
/// Mandate PDA: [MANDATE_SEED, authority, sha256(terms)]. The authority is a
/// seed so nobody can pre-create another authority's mandate.
pub const MANDATE_SEED: &[u8] = b"mandate";

/// The first byte of a mandate account.
pub const MANDATE_TAG: u8 = 1;

/// Self-CPI event instruction discriminator.
pub const EVENT_DISCRIMINATOR: u8 = 255;

/// Mandate flag: closed before its expiry. The account stays until then, so
/// a signature over the same terms cannot create the mandate again.
pub const REVOKED: u8 = 1;

pub const LEDGER_LEN: usize = 8 + MAX_LIMITS * 8; // 72
/// Fixed header; the canonical terms follow it.
pub const MANDATE_LEN: usize = 2 + LEDGER_LEN + 32 + 2; // 108
