# An AI agent with a wallet it can't overspend

A live product, not a slideshow. A research agent buys on-chain analytics from a paid
API over HTTP 402, and settles every request on chain against a daily budget its owner
approved on her phone. The data is real mainnet data from a Surfpool fork, paid for in
real USDC.

```sh
npm install
npm run dev        # from the repo root: the screen on http://localhost:5173
```

Put the screen on the projector and hand someone a phone on the same Wi-Fi.
- **They scan the code and become Alice.** They get test USDC, then approve "up to
  $1.00 a day, only to Inference API". The agent starts working on its own.
- **It pays for every request with no popups.** The budget drains on both screens in
  real time.
- **Prompt-inject the agent.** Mallory's server tries to collect with the agent's
  approval. The program refuses: the approval only pays Inference API.
- **The agent hits $1.00 and stops.** The program refuses the 21st payment.
  *Skip to tomorrow* reopens the budget.
- **Alice taps Revoke.** The agent is cut off mid-task, and the next payment is
  refused on chain.

No phone at hand: open `/phone` in a narrow window next to the screen.

## How it works

```text
agent ──GET /api/analytics──▶ API: 402 { scheme: "mandate", amount, payTo }
agent ──GET + X-PAYMENT: Alice's signed budget──▶ API
    API checks the budget pays this API, then settles on chain:
    Pull $0.05 (the first one also brings Alice's signature) → 200 + data
```

- **The budget is a signed mandate** (`subscriptionTerms`: at most $1 per day, spendable
  only by the API's key). Alice signs it on her phone with no transaction; the first
  time only, one SPL `Approve` turns Mandates on.
- **The API cannot take more than the budget,** nobody else can spend it, and it
  stops working the moment Alice closes it. The program enforces all three, not
  the agent or the API.

| File | Holds |
|---|---|
| `server.ts` | The paid API and Mallory's server: 402, verify, settle with `@mandate/sdk` |
| `src/screen.tsx` | The agent loop (x402 client), live stats, QR code, prompt injection |
| `src/phone.tsx` | Alice's wallet: fund, approve (sign the budget), revoke (`Close`) |
