// The publication. Every report is fiction written for the demo: none of the
// figures are real.
export type Report = { id: string; issue: number; kicker: string; title: string; dek: string; free?: boolean; minutes: number; body: string[] | null };

export const REPORTS: Report[] = [
    {
        body: [
            'Every payment on a blockchain today is a push. You open your wallet, read a transaction and sign it. That works for a purchase. It fails for anything that repeats, because a repeat needs you there each time.',
            'The usual fix is custody. You hand a service your funds, or a key that can move them, and trust it to take only what you agreed. The agreement lives in a terms-of-service page. The key does not read it.',
            'A scoped approval turns that around. You sign a limit, a destination and an expiry, and the chain enforces all three. The service can collect what you agreed and nothing else. You are paying for this publication that way right now.',
            'Our view: within two years most recurring crypto payments will be pulled under signed limits, not pushed and not held in custody. The rest of this issue looks at who gains and who loses when that happens.',
        ],
        dek: 'Why recurring payments on chain were stuck, and what unsticks them.',
        free: true,
        id: 'pull-payments',
        issue: 41,
        kicker: 'Payments',
        minutes: 4,
        title: 'The end of the push payment',
    },
    {
        body: [
            'An agent that can pay is an agent that can be robbed. The question for anyone deploying one is how much, and by whom.',
            'We modelled three designs. In the first the agent holds a funded key. A single prompt injection loses the balance. In the second a server holds the key and the agent asks it to pay. The loss is capped by whatever the server checks, which in practice is little.',
            'In the third the agent holds no key at all. It carries a signed approval: this much a day, only to this provider. An injected agent can hand that approval to an attacker, and the attacker can do nothing with it, because it only pays the provider.',
            'The third design moves the limit from the application to the settlement layer. That is the only place an attacker cannot talk their way past.',
        ],
        dek: 'Three ways to give software a wallet, ranked by what a prompt injection costs you.',
        id: 'agent-budgets',
        issue: 42,
        kicker: 'Agents',
        minutes: 6,
        title: 'What it costs when your agent is tricked',
    },
    {
        body: [
            'Subscription businesses live on involuntary churn: cards expire, banks decline, and a customer who meant to stay is gone. Card networks lose a few percent of renewals this way every month.',
            'A wallet approval does not expire with a piece of plastic. It fails for one reason only, an empty balance, and it says so on chain. A merchant can see a renewal will fail before it tries.',
            'The trade is that cancelling becomes trivial. The member revokes in their wallet and no retention flow stands in the way. We think that is a feature: the businesses that survive it will be the ones people meant to keep.',
        ],
        dek: 'Wallet approvals remove the failed renewal, and the cancellation maze with it.',
        id: 'churn',
        issue: 43,
        kicker: 'Business models',
        minutes: 5,
        title: 'Churn after the card',
    },
];
