# Solana Intent System

A Solana program that lets a wallet owner permit someone to move its tokens, within
limits, without giving up custody.

There are two things an owner can give, with the same terms:

- A **policy** is a standing permission on chain. The owner creates it with a
  transaction, and its spender uses it again and again.
- An **intent** is one action the owner signs as text. Whoever it permits runs it once.
  Nothing goes on chain first, and the owner sends no transaction.

```text
Policy    Create  →  Pull, Pull, Pull …  →  Close
Intent    sign    →  Fill
```

`Pull` and `Fill` move tokens out of the owner's account as its SPL delegate, inside
every limit. If the terms say what the owner must receive, the same instruction moves it
from the spender to the owner and checks what arrived. Funds stay in the owner's wallet
until then, and there is nothing between the two transfers to trust. Another program
can call `Pull`, for example to collect a payment and update its own state together.

| Use | As | Spender | Out | In |
|---|---|---|---|---|
| Subscription | policy | the merchant | 8 USDC every 30 days | nothing |
| Agent budget | policy | the agent's key | 5 per use, 12 a day, 100 in total | nothing |
| DCA | policy | anyone | 10 USDC every day | at least 0.05 SOL each time |
| Authorize, then capture | intent | the merchant | at most 120 USDC, before Friday | nothing |
| One payment signed in advance | intent | the payee | 100 USDC, before a date | nothing |
| Swap, limit order, Dutch auction | intent | anyone | 100 USDC | at least 0.52 SOL, falling to 0.50 |

## Terms

```text
Terms   { authority, spender: key | anyone, not_before, not_after?, salt, limits, receive? }
Limit   { from, mint, max, per: total | every(seconds) | use }
Receive { to, mint, min, decay?: (t0, t1, min) }
```

A limit is what may go out: "at most `max` of `mint` may leave `from`", counted over the
life of a policy, over fixed windows that start at `not_before`, or over a single pull.
Limits on one account stack: a pull must fit every one of them. What is unused in a
window does not carry over.

`receive` is what must come in: "each use, at least `min` of `mint` arrives in `to`", an
account of the owner. It is a total, not a rate: a spender that takes less still
delivers all of it. With a decay the minimum moves in a straight line between two times.

Terms are valid only if they bind someone: a named spender, or something the owner
receives. A per-use limit alone bounds nothing across pulls, so it needs a limit beside
it that persists.

## What an intent signs

```text
Solana Mandate v1
cluster: localnet
engine: Mand89p7P6okjEKQx2SpwDX6mdb5zAcdpshRafFtv7A
authority: A9XwnWUxXn1HH1MPzxe5MqfYaKvEHtdQMCoDd72QjPLN
SPENDER: anyone
MAY TAKE: at most 100.000000 of mint EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v from 6NSx1jcpyqzHDFHwC7RXm4LZy53gNsMZWpV3P8vr8k4M in total
MUST RECEIVE: at least 0.520000000 of mint So11111111111111111111111111111111111111112 in FAUD3SfhKYyynnZsS8pKVgZ9ea5VQohFqpaxKpAwzEGA for each use, moving to 0.500000000 from 2026-10-01T08:16:00Z to 2026-10-01T08:21:00Z
VALID: from 2026-10-01T08:16:00Z until 2026-10-01T09:16:00Z
SALT: 0
```

The text is an Offchain Message v1. `Fill` takes the terms as binary, renders them back
to this text, and verifies Ed25519 over it in-program. The text is all a wallet has to
show, and a byte of the terms cannot change without changing it
(`every_byte_of_the_terms_is_visible_in_the_text`).

An intent runs once, and the only thing it leaves on chain is one bit: its nonce. The
salt is the nonce. Nonces live in pages of 1,024, one bit for every salt, grouped by
owner and by day of expiry, so intents run in any order and none runs twice. A wallet
hands out salts in sequence (`nextSalt`), which keeps an owner's intents in one page.

An intent must expire. The expiry can be far out, which makes an intent a replacement
for a durable nonce when the action is a token movement: sign now, and the spender lands
it any time before the date, with nothing set up first.

## Instructions

| # | Instruction | Signer | |
|---|---|---|---|
| 0 | `Create` | the owner | put a policy on chain |
| 1 | `Pull` | the spender | take tokens under a policy, and deliver what the owner must receive |
| 2 | `Close` | see below | close an account and return its rent |
| 10 | `Fill` | the spender | run a signed intent, once |
| 11 | `Cancel` | the owner | use up an intent's nonce, so it can never run |

Two account types, each recording who paid its rent. Whoever pays is always a separate
account from whoever signs.

| Account | Holds | Closes |
|---|---|---|
| Policy | the terms, and what each limit has consumed | the owner or the spender, at any time; anyone after its expiry |
| Nonces | 1,024 used-nonce bits, for intents of one owner that expire on one day | anyone, once that day is over |

A policy is created only by a transaction, so closing it is final and the rent returns
at once. A page of nonces closes when every intent it guards has expired.

## Cost

LiteSVM, the instruction alone, one run each:

| | CU |
|---|---|
| `Pull` | 4.4k |
| `Pull` that also delivers to the owner | 6.0k |
| `Create` | 7k to 9k |
| `Close` | 2.1k |
| `Fill` | 65k |
| `Fill` that also delivers to the owner, with a decay | 82k |
| `Cancel` | 2.1k |

`Fill` pays for SHA-512 and Ed25519 in-program, and the cost grows with the text.
`Create`, and the first `Fill` or `Cancel` in a page of nonces, also pay a PDA bump
search, which varies by a few thousand.

## Limits

- Not audited, and its invariants are not model-checked.
- The program is upgradeable and is the delegate of every account that enables it.
- A token account has one delegate. Any other `Approve` on it disables its policies and intents.
- Token accounts only. Native SOL has to be wrapped.
- Enabling a token account is an SPL `Approve`, which is a transaction. Its amount caps
  what every policy and intent on the account can pull in total; the SDK's
  `getEnableInstruction` takes it as an option and approves without a cap when it is
  left out.
- A spender that delivers something to the owner must already hold it.
- The owner receives one token, into one account, and each use delivers all of it. An
  order that should fill in parts is signed as several intents.
- Token-2022 transfer hooks are not forwarded. A transfer fee comes out of what the
  spender receives, never out of the owner beyond the limit; a payment to the owner in a
  fee-bearing token is refused unless all of it arrives.
- `Fill` called from another program is not tested; `Pull` is.
- No wallet implements `solana:signMandate`, and no wallet has been tested signing the
  text as an Offchain Message v1.

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
| [`packages/mandate-core`](packages/mandate-core) | Terms, validity rules, the canonical text and the account layouts, shared by the program and every client |
| [`packages/sdk`](packages/sdk) | `@solana/kit` builders plus `mandate-core` compiled to WebAssembly |
| [`packages/wallet-standard`](packages/wallet-standard) | `solana:signMandate`, the one feature a wallet adds to sign intents |
| [`examples`](examples) | The agent demo and the subscription site, both built on policies |
| [`tests`](tests) | LiteSVM flows, a randomized check of the limits against a reference model, and the canonical-text tests |

