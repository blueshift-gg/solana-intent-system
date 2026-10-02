import { budget, decode, encode, ENGINE_ADDRESS, fetchEpoch, fetchMandate, getBumpEpochInstruction, getEnableInstruction, message, spentAt, subscriptionTerms } from '@mandate/sdk';
import { createKeyPairSignerFromPrivateKeyBytes, type KeyPairSigner, signBytes } from '@solana/kit';
import { useEffect, useState } from 'react';

import { api, b64, DAY, every, now, rpc, send, type Shared, unb64, usd, USDC, usdcAccount, usdcOf } from './chain.ts';

/** Alice's key lives in this phone's browser: a demo wallet, not a custody model. */
async function aliceKey() {
    const stored = localStorage.getItem('mandate-alice');
    const seed = stored ? unb64(stored) : crypto.getRandomValues(new Uint8Array(32));
    localStorage.setItem('mandate-alice', b64(seed));
    return createKeyPairSignerFromPrivateKeyBytes(seed);
}

type View = {
    shared: Shared;
    usdc: bigint;
    enabled: boolean;
    mine: boolean;
    spent: bigint;
    revoked: boolean;
};

export function Phone() {
    const [me, setMe] = useState<KeyPairSigner>();
    const [view, setView] = useState<View>();
    const [busy, setBusy] = useState('');

    useEffect(() => void aliceKey().then(setMe), []);
    useEffect(() => {
        if (!me) return;
        return every(1200, async () => {
            const shared = await api<Shared>('/state');
            const token = await usdcOf(me.address);
            const mine = shared.alice === me.address && !!shared.budget;
            let spent = 0n;
            let revoked = false;
            if (mine) {
                const terms = unb64(shared.budget!.terms);
                const t = decode(terms);
                const state = await fetchMandate(rpc, terms);
                spent = state ? spentAt(state, DAY, shared.clock, budget(t)) : 0n;
                revoked = (await fetchEpoch(rpc, me.address)) !== t.epoch;
            }
            const enabled = !!token && token.delegate.__option === 'Some' && token.delegate.value === ENGINE_ADDRESS;
            setView({ enabled, mine, revoked, shared, spent, usdc: token?.amount ?? 0n });
        });
    }, [me]);

    const act = (label: string, run: () => Promise<unknown>) => async () => {
        setBusy(label);
        try {
            await run();
        } finally {
            setBusy('');
        }
    };

    const fund = act('Getting test USDC', () => api('/fund', { address: me!.address }));

    const approve = act('Approving', async () => {
        const account = await usdcAccount(me!.address);
        // Once per token: let Mandates use this USDC, $10 across every approval
        if (!view!.enabled) await send(me!, [getEnableInstruction({ account, amount: 10_000_000n, owner: me! })]);
        // An expiry is the default for a signed approval; `until revoked` would be an explicit opt-in
        const start = await now();
        const terms = encode(subscriptionTerms({
            account,
            amount: BigInt(view!.shared.perDay),
            end: start + 30 * DAY,
            epoch: await fetchEpoch(rpc, me!.address),
            merchant: view!.shared.provider,
            merchantAccount: view!.shared.providerUsdc,
            mint: USDC,
            period: DAY,
            start,
            subscriber: me!.address,
        }));
        // A signature, not a transaction: the exact text the program re-renders
        const signature = await signBytes(me!.keyPair.privateKey, message(terms, { [USDC]: 6 }));
        await api('/budget', { signature: b64(signature), terms: b64(terms) });
    });

    const revoke = act('Revoking', async () => send(me!, [await getBumpEpochInstruction({ authority: me!, payer: me! })]));

    if (!me || !view) return <main className="phone-page"><p className="quiet">Opening Alice’s wallet…</p></main>;
    const perDay = BigInt(view.shared.perDay);
    const requests = Number(view.spent / BigInt(view.shared.price));
    const active = view.mine && !view.revoked;

    return (
        <main className="phone-page">
            <header className="phone-top">
                <span className="brand">Alice’s wallet</span>
                <span className="mono">{me.address.slice(0, 4)}…{me.address.slice(-4)}</span>
            </header>
            <section className="balance-block">
                <span className="balance">{usd(view.usdc)}</span>
                <span className="quiet">USDC</span>
            </section>

            {view.usdc === 0n && !view.mine ? (
                <button type="button" className="big" disabled={!!busy} onClick={fund}>{busy || 'Get 25 test USDC'}</button>
            ) : !active ? (
                <section className="request">
                    <span className="from">Research agent</span>
                    <h1>{view.revoked ? 'Give the agent a new budget?' : 'Your agent asks for a budget'}</h1>
                    <div className="clause take"><span>May spend</span><b>Up to {usd(perDay)} a day</b></div>
                    <div className="clause gain"><span>Only if</span><b>Every cent reaches Inference API</b></div>
                    <p className="quiet">{view.enabled
                        ? 'A signature, not a transaction: no fee. Revoke any time.'
                        : 'First time only: one transaction lets Mandates use your USDC, up to $10 in total. After that, every budget is just a signature.'}</p>
                    <button type="button" className="big go" disabled={!!busy} onClick={approve}>{busy || 'Approve'}</button>
                </section>
            ) : (
                <section className="live">
                    <span className="from">Research agent · working</span>
                    <div className="spent"><b>{usd(view.spent)}</b><span>of {usd(perDay)} today</span></div>
                    <div className="meter"><i style={{ width: `${Number((view.spent * 100n) / perDay)}%` }} /></div>
                    <p className="quiet">{requests} paid request{requests === 1 ? '' : 's'} today. No popups: the budget is the approval.</p>
                    <button type="button" className="big stop" disabled={!!busy} onClick={revoke}>{busy || 'Revoke'}</button>
                </section>
            )}
        </main>
    );
}
