# `solana:signMandate`

The Wallet Standard feature a mandate-aware wallet implements. It's written the way it would
land in `@solana/wallet-standard-features`, following the shape of
`solana:signOffchainMessage`, but it lives in this repo.

## Summary

Adds `solana:signMandate`. A dapp passes canonical Mandate terms, never text. The wallet
decodes and validates them, reads mint decimals from its own RPC, renders the canonical
text itself, shows it with the worst case first, and signs it as an Offchain Message v1.

The output type is `SolanaSignOffchainMessageOutput`, byte for byte. Verifiers and the
on-chain program need nothing new.

```ts
const [{ signature }] = await wallet.features['solana:signMandate'].signMandate({ account, terms });
```

## Why a feature, when `solana:signOffchainMessage` can already sign the text

1. **The wallet renders; the dapp doesn't.** With `signOffchainMessage` the dapp supplies
   the text. The program still rejects any text that isn't the canonical rendering, so
   nothing unsafe gets through. But only a wallet that renders from bytes can show a
   structured approval it computed itself, including decimals it read from chain.
2. **Capability detection.** A dapp can tell whether the wallet understands Mandates and
   pick the right path:

| Wallet supports | Dapp does |
|---|---|
| `solana:signMandate` | sends terms; the wallet shows an approval sheet |
| only `solana:signOffchainMessage` (v1) | renders the canonical text itself; the wallet shows it raw |
| neither | stores the mandate on chain with `solana:signAndSendTransaction` (`CreateMandate`) |

Every row produces a mandate the same program enforces. Only the display differs.

## Wallet requirements

- Reject terms that don't decode as canonical, valid terms for the account's cluster.
- Reject when `account` is not the terms' authority.
- Render with the Mandate canonical renderer and sign
  `"\xffsolana offchain" ‖ 0x01 ‖ 0x01 ‖ authority ‖ text`.
- Show the `SPENDER`, every `MAY TAKE` line and the `PRICE` line.
- Refuse terms that are valid `until revoked`: a signed intent must expire.

The enabling `Approve`, and a mandate's `Create` and `Close`, are ordinary transactions, so
they need no feature.

```sh
npm install && npm run typecheck
```
