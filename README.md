# Solana Intent System

A Solana program that enforces signed intents over SPL token accounts.

An intent states what may leave one token account, and either where it may go or what
must arrive in another. The owner signs it as text, or with a transaction. Anyone can
execute it.

```text
A payment      Open
An exchange    Open  →  any instructions  →  Close
```

`Open` pulls up to the stated limit as the account's SPL delegate. A payment may only go
to the accounts the intent names, so it is one instruction and any program can call it.
An exchange snapshots every balance it names, and `Close` fails the transaction if they
are wrong. Funds stay in the owner's wallet until `Open`, and nothing in between is
trusted.

## What is signed

```text
Solana Mandate v1
cluster: localnet
engine: Mand89p7P6okjEKQx2SpwDX6mdb5zAcdpshRafFtv7A
authority: A9XwnWUxXn1HH1MPzxe5MqfYaKvEHtdQMCoDd72QjPLN
[0] MAY PAY: at most 8.000000 of mint EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v from 6NSx1jcpyqzHDFHwC7RXm4LZy53gNsMZWpV3P8vr8k4M to FAUD3SfhKYyynnZsS8pKVgZ9ea5VQohFqpaxKpAwzEGA, refilling over 30d
EXECUTOR: anyone
VALID: from 2026-10-01T08:16:00Z until 2027-10-01T08:16:00Z
REPLAY: any number of times
EPOCH: 0
```

This is a subscription: 8 USDC, refilling over 30 days, payable only to one account, for a
year. The text is an Offchain Message v1. Whoever collects first brings the signature
and the terms, 169 bytes of binary; the program renders them back to this text, verifies
Ed25519 over it in-program, and stores the terms. Every later collection is a single
instruction with no signature in it. The text is the only thing a wallet has to show,
and a byte of the terms cannot change without changing it
(`every_byte_of_the_terms_is_visible_in_the_text`).

## Terms

```text
Terms   { authority, executor: any | key, not_before, not_after, once, epoch, takes, requires }
Take    { from, mint, max, refill: never | over(period) | each use, to: any | accounts }
Require { target, mint, owner, bound }
Bound   = const | linear(t0, v0, t1, v1) | ratio(of, num, den)
```

A take is a spending limit on one of the owner's token accounts, and the only thing that
permits a pull. A requirement is an amount that must arrive in a token account between
`Open` and `Close`.

| Refill | The limit applies |
|---|---|
| `never` | in total, across every execution |
| `over` | to a budget that refills linearly over `period` seconds |
| `each use` | to a single execution |

Takes on one account stack, so `100 a day`, `10 per use` and `1,000 in total` are three
lines that all hold. With `once`, the intent runs a single time. A per-use limit alone
bounds nothing across executions, so it is only valid next to one that persists, or
with `once`.

Requirements are summed per account across every intent in the transaction, so one
deposit cannot satisfy two intents and two intents can settle against each other.

## Instructions

| # | Instruction | Signer | |
|---|---|---|---|
| 0 | `CreateMandate` | authority, or anyone holding its signature | put an intent on chain |
| 1 | `RevokeMandate` | authority | revoke one intent, on chain or only signed |
| 2 | `BumpEpoch` | authority | revoke every intent |
| 20 | `Open` | executor | check, pull, and for an exchange snapshot |
| 21 | `Close` | executor | check an exchange's outcomes, end the session |
| 22 | `CloseMandate` | anyone | return the rent of an intent that has expired or whose epoch has moved |

Whoever pays an account's rent is always a separate account from whoever signs, and gets
it back when the account closes.

An exchange's `Open` and `Close` must be top-level. The first `Open` reads the
instructions sysvar and requires exactly one `Close` after it. A payment has no `Close`
and may be called by another program; it cannot share a transaction with an exchange.

An intent's account is its own tombstone. Used up or revoked, it stays until the intent
expires or the owner bumps its epoch, so no signature for the same terms can create it
again. That holds its rent until then, which is one more reason to sign an expiry.

## Cost

LiteSVM, whole transaction:

| | CU |
|---|---|
| Payment, `Open` | 5.4k |
| Signed payment, first time: `CreateMandate` + `Open` | 75k |
| Exchange between two intents on chain, `Open` + `Open` + `Close` | 19k |
| Signed swap: `CreateMandate`, `Open`, the solver's transfer, `Close` | 92k to 106k |

The signature is verified once, with SHA-512 and Ed25519 in-program. The spread is PDA
bump search.

## Limits

- Not audited, and its invariants are not model-checked.
- The program is upgradeable and is the delegate of every account that enables it.
- A token account has one delegate. Any other `Approve` on it disables its intents.
- Token accounts only. Native SOL has to be wrapped.
- Enabling a token account is an SPL `Approve`, which is a transaction.
- Token-2022 transfer hooks are not forwarded. A transfer fee comes out of what the
  destination receives, never out of the payer beyond the limit.
- No wallet implements `solana:signMandate`. Signing falls back to an offchain message.

## Build

```sh
cargo build-sbf --manifest-path program/Cargo.toml --features localnet
cargo build-sbf --manifest-path tests/caller/Cargo.toml    # a test fixture that calls by CPI
cargo test --workspace

npm install
npm run dev                  # examples/agents on a Surfpool mainnet fork
npm run dev:subscriptions    # examples/subscriptions
```

## Layout

| Path | Holds |
|---|---|
| [`program`](program) | The engine |
| [`packages/mandate-core`](packages/mandate-core) | Terms, validity rules, the canonical text and account layouts, shared by the program and every client |
| [`packages/sdk`](packages/sdk) | `@solana/kit` builders plus `mandate-core` compiled to WebAssembly |
| [`packages/wallet-standard`](packages/wallet-standard) | `solana:signMandate`, the one feature a wallet adds |
| [`examples`](examples) | The agent demo and the subscription site |
| [`tests`](tests) | LiteSVM flows, a randomized check of the limits against a reference model, and the canonical-text tests |

The code calls an intent a mandate.
