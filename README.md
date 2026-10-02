# Solana Intent System

A Solana program that lets a wallet owner permit someone to move its tokens, within
limits, without giving up custody.

A permission (the code calls it a mandate) names a spender and one or more limits on the
owner's token accounts. It can also set a price: what the owner must receive for what is
taken. The spender uses it with one instruction.

```text
Create  →  Pull, Pull, Pull …  →  Close
```

`Pull` moves tokens out of the owner's account as its SPL delegate, inside every limit.
If there is a price, the same instruction moves the spender's payment to the owner and
checks what arrived. Funds stay in the owner's wallet until a pull, and there is nothing
between the two transfers to trust. Any program can call it.

| Use | Spender | Limits | Price |
|---|---|---|---|
| Subscription | the merchant | 8 USDC every 30 days | none |
| Agent budget | the agent's key | 5 per use, 12 a day, 100 in total | none |
| One payment signed in advance | the payee | 100 in total, with an expiry | none |
| Limit order, filled in parts | anyone | 100 USDC in total | at least 0.005 SOL per USDC |
| Dutch auction | anyone | 100 USDC in total | a price that falls over five minutes |
| DCA | anyone | 10 USDC every day | at least 0.005 SOL per USDC |

## Terms

```text
Terms { authority, spender: key | anyone, not_before, not_after?, salt, limits, price? }
Limit { from, mint, max, per: total | every(seconds) | use }
Price { to, mint, num, den, decay?: (t0, t1, num) }
```

A limit is "at most `max` of `mint` may leave `from`", counted over the life of the
permission, over fixed windows that start at `not_before`, or over a single pull. Limits
on one account stack: a pull must fit every one of them. What is unused in a window does
not carry over.

A price is "for every `den` taken, at least `num` of `mint` arrives in `to`", an account
of the owner. With a decay the rate moves in a straight line between two times.

Terms are valid only if they bind someone: a named spender, or a price the owner is
paid. A per-use limit alone bounds nothing across pulls, so it needs a limit beside it
that persists. The `salt` tells apart permissions whose terms are otherwise identical.

## Two ways to consent

The owner signs the `Create` transaction, or signs this text and lets anyone bring the
signature:

```text
Solana Mandate v1
cluster: localnet
engine: Mand89p7P6okjEKQx2SpwDX6mdb5zAcdpshRafFtv7A
authority: A9XwnWUxXn1HH1MPzxe5MqfYaKvEHtdQMCoDd72QjPLN
SPENDER: anyone
MAY TAKE: at most 100.000000 of mint EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v from 6NSx1jcpyqzHDFHwC7RXm4LZy53gNsMZWpV3P8vr8k4M in total
PRICE: at least 0.005200000 of mint So11111111111111111111111111111111111111112 to FAUD3SfhKYyynnZsS8pKVgZ9ea5VQohFqpaxKpAwzEGA for every 1.000000 taken, moving to 0.005000000 from 2026-10-01T08:16:00Z to 2026-10-01T08:21:00Z
VALID: from 2026-10-01T08:16:00Z until 2026-10-01T09:16:00Z
SALT: 8970456561443998011
```

The text is an Offchain Message v1. `Create` takes the terms as binary, renders them
back to this text, and verifies Ed25519 over it in-program, once. After that a signed
permission and one created by transaction are the same account. The text is all a wallet
has to show, and a byte of the terms cannot change without changing it
(`every_byte_of_the_terms_is_visible_in_the_text`).

A signature can only stand behind terms that expire. The expiry can be far out, so a
signed permission also does the job of a durable nonce for token movements: sign now,
and the spender lands it any time before the date.

## Instructions

| # | Instruction | Signer | |
|---|---|---|---|
| 0 | `Create` | the owner, or anyone holding its signature | put a permission on chain |
| 1 | `Pull` | the spender | take tokens within the limits, and pay the price if there is one |
| 2 | `Close` | the owner or the spender; anyone after the expiry | end it and return the rent |

There is one account type, the permission, at a PDA of the owner and the hash of the
terms. It records who paid its rent, which is always a separate account from whoever
signs, and `Close` pays it back.

A permission that never expires closes at once. One that expires may have a signature
behind it, so closing it early only marks it revoked; it stays until its expiry, when
anyone can close it and the rent returns. A signature can therefore never bring back
something that was revoked.

## Cost

One run each on a Surfpool mainnet fork, whole transaction:

| | CU |
|---|---|
| `Pull` | 4.5k |
| `Pull` with a price | 6.1k |
| `Create` by transaction | 6.0k |
| `Create` by signature, with the first `Pull` | 74k to 80k |
| `Close` | 2.3k |

The signature costs SHA-512 and Ed25519 in-program, once. `Create` also pays a PDA bump
search, which varies by a few thousand.

## Limits

- Not audited, and its invariants are not model-checked.
- The program is upgradeable and is the delegate of every account that enables it.
- A token account has one delegate. Any other `Approve` on it disables its permissions.
- Token accounts only. Native SOL has to be wrapped.
- Enabling a token account is an SPL `Approve`, which is a transaction. Its amount caps
  what every permission on the account can pull in total; the SDK's
  `getEnableInstruction` takes it as an option and approves without a cap when it is
  left out.
- A spender that fills a priced permission must already hold what it pays.
- A price pays one token into one account of the owner.
- Token-2022 transfer hooks are not forwarded. A transfer fee comes out of what the
  spender receives, never out of the owner beyond the limit; a price paid in a
  fee-bearing token is refused unless the owner receives all of it.
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
| [`packages/mandate-core`](packages/mandate-core) | Terms, validity rules, the canonical text and the account layout, shared by the program and every client |
| [`packages/sdk`](packages/sdk) | `@solana/kit` builders plus `mandate-core` compiled to WebAssembly |
| [`packages/wallet-standard`](packages/wallet-standard) | `solana:signMandate`, the one feature a wallet adds |
| [`examples`](examples) | The agent demo and the subscription site |
| [`tests`](tests) | LiteSVM flows, a randomized check of the limits against a reference model, and the canonical-text tests |

