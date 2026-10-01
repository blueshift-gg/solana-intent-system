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

/// Takes plus requirements in one mandate.
pub const MAX_ASSERTS: usize = 8;
/// 9999-12-31T23:59:59Z: the last instant the canonical text can render.
pub const MAX_TIME: i64 = 253_402_300_799;
pub const MAX_ENTRIES: usize = 16;

pub const TOKEN_PROGRAM: Pubkey = decode_32_const("TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA");
pub const TOKEN_2022_PROGRAM: Pubkey =
    decode_32_const("TokenzQdBNbLqP5VEhdkAS6EPFLC1PHnBqCXEpPxuEb");

/// Engine PDA: [ENGINE_SEED]. The SPL delegate of every enabled token account
/// and the signer of every event; the test suite re-derives it.
pub const ENGINE_SEED: &[u8] = b"engine";
pub const ENGINE: Pubkey = decode_32_const("6NpP2w9pBwSWBkQ7yNPYo5ruPY47goYB8DNsjBuKGyHp");
pub const ENGINE_BUMP: u8 = 254;
/// Mandate PDA: [MANDATE_SEED, authority, mandate_id]. The authority is a
/// seed so nobody can pre-create another authority's mandate.
pub const MANDATE_SEED: &[u8] = b"mandate";
/// Revocation epoch of an authority's mandates: [EPOCH_SEED, authority].
pub const EPOCH_SEED: &[u8] = b"epoch";
/// Per-executor session: [SESSION_SEED, executor].
pub const SESSION_SEED: &[u8] = b"session";

/// Account tags: the first byte of every account, one per type.
pub const MANDATE_TAG: u8 = 1;
pub const EPOCH_TAG: u8 = 3;
pub const SESSION_TAG: u8 = 4;

/// Self-CPI event instruction discriminator.
pub const EVENT_DISCRIMINATOR: u8 = 255;

/// Mandate flags.
pub const DONE: u8 = 1;
pub const REVOKED: u8 = 2;

/// Session entry kinds: a summed bound, or a snapshot no mandate has bounded
/// yet (a pull destination), which `Close` skips.
pub const BOUND: u8 = 0;
pub const UNBOUND: u8 = 1;

pub const LEDGER_LEN: usize = 8 + MAX_ASSERTS * 8; // 72
/// Fixed header; the canonical terms follow it.
pub const MANDATE_LEN: usize = 2 + LEDGER_LEN + 8 + 4 + 3 * 32 + 2; // 184
pub const EPOCH_LEN: usize = 1 + 32 + 4; // 37
pub const ENTRY_LEN: usize = 3 * 32 + 1 + 2 * 16; // 129
pub const SESSION_LEN: usize = 3 + 32 + MAX_ENTRIES * ENTRY_LEN; // 2099
