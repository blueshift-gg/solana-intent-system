# Fathom: a subscription business on Mandate

A product, not a console. Fathom is a (fictional) weekly research publication with a
paywall. Readers subscribe from their own wallet in USDC, the publisher's server collects
each month without them, and they cancel in one tap. It is written the way it would be
if the Mandate program were live and wallets shipped `solana:signMandate`.

```sh
npm install
npm run dev:subscriptions    # from the repo root: program, Surfpool, the site on http://localhost:5173
```

Try it:
- **Open a members-only report** and subscribe. The Demo Wallet shows the approval in its
  own words, with the exact signed text one tap away.
- **A wallet that connects gets test SOL and 50 USDC** on the local fork (dev builds only).
- **Skip 30 days** is in the footer. After a skip the server's billing run collects the
  next month by itself.
- **Cancel** on the account page. Access lasts to the end of the paid period and nothing
  more can be charged.

## What each side does

| Who | Does | With |
|---|---|---|
| Reader | Approves "Fathom may take at most $8 every 30 days" | One signature. The first time only, one SPL `Approve` turns Mandates on for USDC |
| Fathom's server | Checks the approval matches the plan, charges the first month, then every 30 days | one `Pull` with its own key, plus `Create` the first time if the reader signed; the program refuses anything the reader didn't approve |
| Reader | Cancels | `Close`; the server sees it on chain |

The server keeps the member list, as any business does, but it is not the authority: it
cannot charge more, more often, or to another account, and it learns of a cancellation
from the chain.

## Wallets

The site is an ordinary Wallet Standard dapp and picks the path from the wallet's features:

| Wallet has | The site |
|---|---|
| `solana:signMandate` | sends the terms; the wallet renders and signs them. No transaction, no fee |
| `solana:signOffchainMessage` | renders the canonical text itself and has the wallet sign it; the wallet shows it raw |
| only `solana:signTransaction` | stores the same terms on chain with `CreateMandate`, in one transaction |

`src/wallet.ts` is a dev-only wallet that lives in the page, so nothing needs installing.
It implements `solana:signMandate` as a real wallet would: it decodes the terms, reads the
decimals from chain, and renders the text itself. Open the site with `?offchain` or
`?basic` to get the same wallet without the feature and see each fallback.

| File | Holds |
|---|---|
| `server.ts` | The publisher's backend: plans, the member list, the billing run, the paywall |
| `reports.ts` | The publication's content |
| `src/app.tsx` | The site: reports, pricing, checkout, account |
| `src/wallet.ts` | The Demo Wallet and its approval sheet |
| `src/chain.ts` | RPC, sending transactions, formatting |
