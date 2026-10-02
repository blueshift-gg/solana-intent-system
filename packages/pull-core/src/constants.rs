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

/// Limits in one policy.
pub const MAX_LIMITS: usize = 8;
/// 9999-12-31T23:59:59Z: the last instant the canonical text can render.
pub const MAX_TIME: i64 = 253_402_300_799;

pub const TOKEN_PROGRAM: Pubkey = decode_32_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const TOKEN_2022_PROGRAM: Pubkey =
    decode_32_const("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

/// Engine PDA: [ENGINE_SEED]. The SPL delegate of every enabled token account
/// and the signer of every event; the test suite re-derives it.
pub const ENGINE_SEED: &[u8] = b"engine";
pub const ENGINE: Pubkey = decode_32_const("58eSE1WJDvzrwz7BZ23sbsDiqa75pxetzkyRyUcPwb6E");
pub const ENGINE_BUMP: u8 = 255;
/// Policy PDA: [POLICY_SEED, authority, sha256(terms)]. The authority is a
/// seed so nobody can pre-create another authority's policy.
pub const POLICY_SEED: &[u8] = b"policy";

/// Used nonces of signed intents: [NONCES_SEED, authority, day, page], both
/// little-endian. `day` is the intent's expiry in whole days: every intent in
/// a page is dead once that day is over, so the page can then be closed.
/// `page` is the intent's salt divided by `NONCE_BITS`, and its bit in the
/// page is the remainder: every salt has its own bit.
pub const NONCES_SEED: &[u8] = b"nonces";
pub const NONCE_DAY: i64 = 86_400;
/// Nonces in one page.
pub const NONCE_BITS: usize = 1024;

/// Account tags: the first byte of every account, one per type.
pub const POLICY_TAG: u8 = 1;
pub const NONCES_TAG: u8 = 2;

/// Self-CPI event instruction discriminator.
pub const EVENT_DISCRIMINATOR: u8 = 255;

pub const LEDGER_LEN: usize = 8 + MAX_LIMITS * 8; // 72
/// Fixed header; the canonical terms follow it.
pub const POLICY_LEN: usize = 1 + LEDGER_LEN + 32 + 2; // 107
pub const NONCES_LEN: usize = 1 + 2 * 32 + 2 * 8 + NONCE_BITS / 8; // 209
