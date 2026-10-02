//! Test fixture: LiteSVM with the Mandate program, a USDC-like and a SOL-like
//! mint, funded wallets that enabled the engine, and a builder per instruction.
//!
//! Build the program and the CPI caller fixture first:
//! `cargo build-sbf --manifest-path program/Cargo.toml --features localnet`
//! `cargo build-sbf --manifest-path tests/caller/Cargo.toml`.

use ed25519_dalek::{Signer as _, SigningKey};
use litesvm::{types::TransactionResult, LiteSVM};
use litesvm_token::{
    get_spl_account, spl_token, Approve, CreateAssociatedTokenAccount, CreateMint, MintTo,
};
use mandate_core::render::{envelope, render};
use mandate_core::terms::{Bound, Refill, Require, Seq, Take, Terms};
use mandate_core::{constants::*, errors::MandateError};
use sha2::{Digest, Sha256};
use solana_address::Address;
use solana_clock::Clock;
use solana_instruction::{AccountMeta, Instruction};
use solana_keypair::Keypair;
use solana_signer::Signer;
use solana_transaction::Transaction;

pub const PROGRAM: Address = Address::new_from_array(mandate_core::ID);
/// Where the fixture loads `tests/caller`, a program that forwards to Mandate.
pub const CALLER: Address = Address::from_str_const("Ca11er1111111111111111111111111111111111111");
pub const ENGINE_KEY: Address = Address::new_from_array(ENGINE);
pub const TOKEN: Address = litesvm_token::TOKEN_ID;
pub const SYSTEM: Address = Address::new_from_array([0; 32]);
pub const INSTRUCTIONS: Address =
    Address::from_str_const("Sysvar1nstructions1111111111111111111111111");
/// 2026-09-21T14:13:20Z.
pub const NOW: i64 = 1_790_000_000;
pub const USDC: u64 = 1_000_000;
pub const SOL: u64 = 1_000_000_000;

pub struct Wallet {
    pub key: Keypair,
    pub usdc: Address,
    pub sol: Address,
}

impl Wallet {
    pub fn address(&self) -> Address {
        self.key.pubkey()
    }
}

pub struct Fixture {
    pub svm: LiteSVM,
    pub usdc: Address,
    pub sol: Address,
    mint_authority: Keypair,
}

impl Default for Fixture {
    fn default() -> Self {
        Self::new()
    }
}

impl Fixture {
    pub fn new() -> Self {
        let mut svm = LiteSVM::new();
        let so = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../target/deploy/mandate_program.so"
        );
        svm.add_program_from_file(PROGRAM, so)
            .expect("build the program first");
        let caller = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../target/deploy/mandate_caller.so"
        );
        svm.add_program_from_file(CALLER, caller)
            .expect("build tests/caller first");
        let mint_authority = Keypair::new();
        svm.airdrop(&mint_authority.pubkey(), 10 * SOL).unwrap();
        let usdc = CreateMint::new(&mut svm, &mint_authority)
            .decimals(6)
            .send()
            .unwrap();
        let sol = CreateMint::new(&mut svm, &mint_authority)
            .decimals(9)
            .send()
            .unwrap();
        let mut f = Self {
            svm,
            usdc,
            sol,
            mint_authority,
        };
        f.set_time(NOW);
        f
    }

    /// A funded wallet whose token accounts delegate to the engine.
    pub fn wallet(&mut self, usdc: u64, sol: u64) -> Wallet {
        let wallet = self.fresh_wallet(usdc, sol);
        self.enable(&wallet, &wallet.usdc, u64::MAX);
        self.enable(&wallet, &wallet.sol, u64::MAX);
        wallet
    }

    /// A funded wallet that has not enabled mandates yet.
    pub fn fresh_wallet(&mut self, usdc: u64, sol: u64) -> Wallet {
        let key = Keypair::new();
        let Self {
            svm,
            mint_authority,
            ..
        } = self;
        svm.airdrop(&key.pubkey(), 10 * SOL).unwrap();
        let mut account = |mint: Address, amount: u64| {
            let ata = CreateAssociatedTokenAccount::new(svm, &key, &mint)
                .send()
                .unwrap();
            MintTo::new(svm, mint_authority, &mint, &ata, amount)
                .send()
                .unwrap();
            ata
        };
        let (usdc, sol) = (account(self.usdc, usdc), account(self.sol, sol));
        Wallet { key, usdc, sol }
    }

    /// A funded keypair with no token accounts: a solver that holds nothing.
    pub fn payer(&mut self) -> Keypair {
        let key = Keypair::new();
        self.svm.airdrop(&key.pubkey(), 10 * SOL).unwrap();
        key
    }

    /// The one-time setup for a token: approve the engine as delegate. `cap`
    /// is the budget every mandate on this account shares; `u64::MAX` for none.
    pub fn enable(&mut self, wallet: &Wallet, account: &Address, cap: u64) {
        Approve::new(&mut self.svm, &wallet.key, &ENGINE_KEY, account, cap)
            .send()
            .unwrap();
    }

    /// What the engine may still pull from `account`, across every mandate.
    pub fn allowance(&self, account: &Address) -> u64 {
        get_spl_account::<spl_token::state::Account>(&self.svm, account)
            .unwrap()
            .delegated_amount
    }

    pub fn balance(&self, account: &Address) -> u64 {
        get_spl_account::<spl_token::state::Account>(&self.svm, account)
            .unwrap()
            .amount
    }

    pub fn set_time(&mut self, unix_timestamp: i64) {
        let mut clock: Clock = self.svm.get_sysvar();
        clock.unix_timestamp = unix_timestamp;
        self.svm.set_sysvar(&clock);
    }

    /// Send `ixs`, paid by the first signer.
    #[allow(clippy::result_large_err)] // LiteSVM's own result type
    pub fn send(&mut self, ixs: &[Instruction], signers: &[&Keypair]) -> TransactionResult {
        self.svm.expire_blockhash();
        let tx = Transaction::new_signed_with_payer(
            ixs,
            Some(&signers[0].pubkey()),
            signers,
            self.svm.latest_blockhash(),
        );
        self.svm.send_transaction(tx)
    }

    pub fn decimals(&self) -> impl Fn(&[u8; 32]) -> Result<u8, MandateError> + '_ {
        |mint| match Address::new_from_array(*mint) {
            m if m == self.usdc => Ok(6),
            m if m == self.sol => Ok(9),
            _ => Err(MandateError::MissingAccount),
        }
    }
}

pub fn pda(seeds: &[&[u8]]) -> Address {
    Address::find_program_address(seeds, &PROGRAM).0
}

pub fn encode(terms: &Terms) -> Vec<u8> {
    let mut bytes = Vec::new();
    terms.write(&mut bytes);
    bytes
}

pub fn mandate_id(bytes: &[u8]) -> [u8; 32] {
    Sha256::digest(bytes).into()
}

/// The canonical text a wallet shows.
pub fn text(terms: &Terms, decimals: impl Fn(&[u8; 32]) -> Result<u8, MandateError>) -> String {
    let mut out = Vec::new();
    render(terms, decimals, &mut out).unwrap();
    String::from_utf8(out).unwrap()
}

/// What a wallet signs through `solana:signOffchainMessage`.
pub fn sign(
    terms: &Terms,
    key: &Keypair,
    decimals: impl Fn(&[u8; 32]) -> Result<u8, MandateError>,
) -> [u8; 64] {
    let mut message = Vec::new();
    envelope(terms.authority, &mut message);
    render(terms, decimals, &mut message).unwrap();
    let secret: [u8; 32] = key.to_bytes()[..32].try_into().unwrap();
    SigningKey::from_bytes(&secret).sign(&message).to_bytes()
}

/// "At most `max` may leave `from`", to wherever the executor sends it.
pub fn take<'a>(from: &'a [u8; 32], mint: &'a [u8; 32], max: u64, refill: Refill) -> Take<'a> {
    Take {
        from,
        mint,
        max,
        refill,
        to: &[],
    }
}

/// "At most `max` may leave `from`, and only to `to`."
pub fn pay<'a>(
    from: &'a [u8; 32],
    mint: &'a [u8; 32],
    max: u64,
    refill: Refill,
    to: &'a [[u8; 32]],
) -> Take<'a> {
    Take {
        to,
        ..take(from, mint, max, refill)
    }
}

/// "`target` must gain at least `bound`."
pub fn gain<'a>(
    target: &'a [u8; 32],
    mint: &'a [u8; 32],
    owner: &'a [u8; 32],
    bound: Bound,
) -> Require<'a> {
    Require {
        target,
        mint,
        owner,
        bound,
    }
}

pub fn terms<'a>(
    authority: &'a [u8; 32],
    once: bool,
    not_after: Option<i64>,
    takes: &'a [Take<'a>],
    requires: &'a [Require<'a>],
) -> Terms<'a> {
    Terms {
        cluster: CLUSTER,
        authority,
        executor: None,
        not_before: NOW,
        not_after,
        once,
        epoch: 0,
        salt: 0,
        takes: Seq::List(takes),
        requires: Seq::List(requires),
    }
}

/// Every Open/Close carries its extra accounts: token accounts writable,
/// mints and the token program read-only.
pub fn extra(writable: &[Address], readonly: &[Address]) -> Vec<AccountMeta> {
    let w = writable.iter().map(|a| AccountMeta::new(*a, false));
    let r = readonly
        .iter()
        .map(|a| AccountMeta::new_readonly(*a, false));
    w.chain(r).collect()
}

/// `(from, to, amount)` with `from`/`to` resolved to indices into `extra`.
fn pulls(extra: &[AccountMeta], pulls: &[(Address, Address, u64)]) -> Vec<u8> {
    let index = |key: &Address| extra.iter().position(|m| m.pubkey == *key).unwrap() as u8;
    let mut data = vec![pulls.len() as u8];
    for (from, to, amount) in pulls {
        data.extend([index(from), index(to)]);
        data.extend(amount.to_le_bytes());
    }
    data
}

pub fn mandate_pda(authority: &Address, bytes: &[u8]) -> Address {
    pda(&[MANDATE_SEED, authority.as_ref(), &mandate_id(bytes)])
}

/// Put a mandate on chain, rent from `payer`. With a `signature` over the
/// text the authority signs nothing here; the text names `mints`.
pub fn create_mandate(
    authority: &Address,
    payer: &Address,
    bytes: &[u8],
    signature: Option<(&[u8; 64], &[Address])>,
) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new_readonly(*authority, signature.is_none()),
        AccountMeta::new(*payer, true),
        AccountMeta::new(mandate_pda(authority, bytes), false),
        AccountMeta::new_readonly(pda(&[EPOCH_SEED, authority.as_ref()]), false),
        AccountMeta::new_readonly(SYSTEM, false),
        AccountMeta::new_readonly(ENGINE_KEY, false),
        AccountMeta::new_readonly(PROGRAM, false),
    ];
    let mut data = [&[0][..], &(bytes.len() as u16).to_le_bytes(), bytes].concat();
    if let Some((signature, mints)) = signature {
        accounts.extend(mints.iter().map(|m| AccountMeta::new_readonly(*m, false)));
        data.extend(signature);
    }
    Instruction {
        program_id: PROGRAM,
        accounts,
        data,
    }
}

/// Execute a mandate; `payer` funds the session of an exchange on first use.
pub fn open(
    executor: &Address,
    payer: &Address,
    authority: &Address,
    bytes: &[u8],
    extra: Vec<AccountMeta>,
    pull: &[(Address, Address, u64)],
) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new_readonly(*executor, true),
        AccountMeta::new(pda(&[SESSION_SEED, executor.as_ref()]), false),
        AccountMeta::new(*payer, true),
        AccountMeta::new(mandate_pda(authority, bytes), false),
        AccountMeta::new_readonly(pda(&[EPOCH_SEED, authority.as_ref()]), false),
        AccountMeta::new_readonly(INSTRUCTIONS, false),
        AccountMeta::new_readonly(SYSTEM, false),
        AccountMeta::new_readonly(ENGINE_KEY, false),
        AccountMeta::new_readonly(PROGRAM, false),
    ];
    let data = [&[20][..], &pulls(&extra, pull)].concat();
    accounts.extend(extra);
    Instruction {
        program_id: PROGRAM,
        accounts,
        data,
    }
}

/// The same instruction, sent to the caller fixture, which forwards it by CPI.
pub fn by_cpi(mut ix: Instruction) -> Instruction {
    ix.program_id = CALLER;
    ix
}

pub fn close(executor: &Address, targets: &[Address]) -> Instruction {
    let mut accounts = vec![
        AccountMeta::new_readonly(*executor, true),
        AccountMeta::new(pda(&[SESSION_SEED, executor.as_ref()]), false),
        AccountMeta::new_readonly(ENGINE_KEY, false),
        AccountMeta::new_readonly(PROGRAM, false),
    ];
    accounts.extend(targets.iter().map(|t| AccountMeta::new_readonly(*t, false)));
    Instruction {
        program_id: PROGRAM,
        accounts,
        data: vec![21],
    }
}

/// The executor's own leg: an ordinary `TransferChecked` it signs.
pub fn transfer(
    from: &Address,
    mint: &Address,
    to: &Address,
    owner: &Address,
    amount: u64,
    decimals: u8,
) -> Instruction {
    spl_token::instruction::transfer_checked(&TOKEN, from, mint, to, owner, &[], amount, decimals)
        .unwrap()
}

/// Reclaim a mandate's rent for whoever paid it, once it can never run again.
pub fn close_mandate(mandate: &Address, payer: &Address, authority: &Address) -> Instruction {
    Instruction {
        program_id: PROGRAM,
        accounts: vec![
            AccountMeta::new(*mandate, false),
            AccountMeta::new(*payer, false),
            AccountMeta::new_readonly(pda(&[EPOCH_SEED, authority.as_ref()]), false),
            AccountMeta::new_readonly(ENGINE_KEY, false),
            AccountMeta::new_readonly(PROGRAM, false),
        ],
        data: vec![22],
    }
}

/// An authority instruction: `[authority, payer, account, accounts…, system,
/// engine, program]`. The authority pays here, and `account` is writable.
pub fn authority_ix(
    discriminator: u8,
    authority: &Address,
    account: &Address,
    accounts: &[Address],
    data: &[u8],
) -> Instruction {
    let mut metas = vec![
        AccountMeta::new_readonly(*authority, true),
        AccountMeta::new(*authority, true),
        AccountMeta::new(*account, false),
    ];
    metas.extend(
        accounts
            .iter()
            .map(|a| AccountMeta::new_readonly(*a, false)),
    );
    metas.extend([
        AccountMeta::new_readonly(SYSTEM, false),
        AccountMeta::new_readonly(ENGINE_KEY, false),
        AccountMeta::new_readonly(PROGRAM, false),
    ]);
    Instruction {
        program_id: PROGRAM,
        accounts: metas,
        data: [&[discriminator][..], data].concat(),
    }
}
