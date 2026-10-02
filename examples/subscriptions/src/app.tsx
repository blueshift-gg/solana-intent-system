import {
    encode,
    ENGINE_ADDRESS,
    fetchPolicy,
    getCloseInstruction,
    getCreateInstruction,
    policyAddress,
    getEnableInstruction,
    subscriptionTerms,
    text,
} from '@mandate/sdk';
import { address, type Address, type TransactionModifyingSigner, type TransactionSigner } from '@solana/kit';
import { useWalletAccountTransactionSigner } from '@solana/react';
import { fetchMaybeToken } from '@solana-program/token';
import { type UiWallet, type UiWalletAccount, useConnect, useWallets } from '@wallet-standard/react';
import { createContext, useCallback, useContext, useEffect, useMemo, useRef, useState } from 'react';

import type { Report } from '../reports.ts';
import { api, ata, b64, DAY, day, now, rpc, send, short, unb64, USDC, usd } from './chain.ts';


type Plan = { id: string; name: string; price: string; period: number; perks: string[] };
type Config = { clock: number; merchant: Address; merchantUsdc: Address; plans: Plan[] };
type Membership = {
    plan: string;
    status: 'active' | 'past_due' | 'cancelled';
    since: number;
    paidThrough: number;
    terms: string;
    reason?: string;
    payments: { amount: string; at: number; signature: string }[];
};
type Page = { name: 'home' } | { name: 'report'; id: string } | { name: 'account' };

/** The connected wallet: who the reader is and what signs for them. */
type Reader = { account: UiWalletAccount; me: Address; signer: TransactionSigner };
const ReaderContext = createContext<Reader | null>(null);

/** Keeps `into` pointing at the wallet's signer. A hook needs its own component, mounted only once connected. */
function Signer({ account, into }: { account: UiWalletAccount; into: { current: TransactionModifyingSigner | null } }) {
    const chain = account.chains.find((c) => c === 'solana:localnet') ?? account.chains[0];
    into.current = useWalletAccountTransactionSigner(account, chain as `solana:${string}`);
    return null;
}

export function Root() {
    const [account, setAccount] = useState<UiWalletAccount | null>(null);
    const latest = useRef<TransactionModifyingSigner | null>(null);
    // Some wallets hand out a new signer on every render. The site holds one that
    // never changes and signs with the latest, so nothing re-renders in a loop.
    const reader = useMemo(() => {
        if (!account) return null;
        const me = address(account.address);
        const signer: TransactionModifyingSigner = { address: me, modifyAndSignTransactions: (...args) => latest.current!.modifyAndSignTransactions(...args) };
        return { account, me, signer };
    }, [account]);
    return (
        <ReaderContext.Provider value={reader}>
            {account ? <Signer account={account} into={latest} /> : null}
            <Site onConnect={setAccount} onDisconnect={() => setAccount(null)} />
        </ReaderContext.Provider>
    );
}

function Site({ onConnect, onDisconnect }: { onConnect: (a: UiWalletAccount) => void; onDisconnect: () => void }) {
    const reader = useContext(ReaderContext);
    const [config, setConfig] = useState<Config>();
    const [reports, setReports] = useState<Report[]>([]);
    const [membership, setMembership] = useState<Membership | null>(null);
    const [page, setPage] = useState<Page>({ name: 'home' });
    const [checkout, setCheckout] = useState<Plan | null>(null);

    const refresh = useCallback(async () => {
        const who = reader ? `?address=${reader.me}` : '';
        setConfig(await api<Config>('/config'));
        setReports(await api<Report[]>(`/reports${who}`));
        setMembership(reader ? await api<Membership | null>(`/account${who}`) : null);
    }, [reader]);
    useEffect(() => {
        // Dev builds: a wallet that connects gets test SOL and USDC on the local fork
        const funded = import.meta.env.DEV && reader ? api('/dev/fund', { address: reader.me }) : Promise.resolve();
        funded.then(refresh).catch(console.error);
        const timer = setInterval(() => refresh().catch(console.error), 4000);
        return () => clearInterval(timer);
    }, [reader, refresh]);

    if (!config) return null;
    const member = !!membership && membership.paidThrough > config.clock;
    const plan = config.plans.find((p) => p.id === membership?.plan);
    const go = (next: Page) => {
        setPage(next);
        scrollTo(0, 0);
    };

    return (
        <>
            <header className="nav">
                <button type="button" className="brand" onClick={() => go({ name: 'home' })}>Fathom</button>
                <nav>
                    <a href="#research" onClick={() => go({ name: 'home' })}>Research</a>
                    {member ? null : <a href="#pricing" onClick={() => go({ name: 'home' })}>Pricing</a>}
                    {reader ? (
                        <button type="button" className="chip" onClick={() => go({ name: 'account' })}>
                            {member ? <i className="live" /> : null}
                            {short(reader.me)}
                        </button>
                    ) : (
                        <SignIn onConnect={onConnect} label="Sign in" />
                    )}
                </nav>
            </header>

            {page.name === 'home' ? (
                <Home config={config} reports={reports} member={member} open={(id) => go({ name: 'report', id })} choose={setCheckout} />
            ) : null}
            {page.name === 'report' ? (
                <ReportPage report={reports.find((r) => r.id === page.id)} plans={config.plans} choose={setCheckout} back={() => go({ name: 'home' })} />
            ) : null}
            {page.name === 'account' ? (
                <Account config={config} membership={membership} plan={plan} refresh={refresh} choose={setCheckout} signOut={() => { onDisconnect(); go({ name: 'home' }); }} />
            ) : null}

            {checkout ? (
                <Checkout
                    config={config}
                    plan={checkout}
                    onConnect={onConnect}
                    close={() => setCheckout(null)}
                    done={async () => {
                        await refresh();
                        setCheckout(null);
                        go({ name: 'account' });
                    }}
                />
            ) : null}

            <footer className="foot">
                <span>© Fathom Research. Reports are fiction written for this demo.</span>
                {import.meta.env.DEV ? <LocalFork refresh={refresh} clock={config.clock} /> : null}
            </footer>
        </>
    );
}

/** Wallets this browser has that can sign a transaction. */
function SignIn({ onConnect, label }: { onConnect: (a: UiWalletAccount) => void; label?: string }) {
    const wallets = useWallets().filter((w) => w.features.includes('solana:signTransaction') && w.features.includes('standard:connect'));
    if (!wallets.length) return <span className="quiet">No Solana wallet found</span>;
    return (
        <span className="wallets">
            {wallets.map((w) => (
                <WalletButton key={w.name} wallet={w} onConnect={onConnect} label={wallets.length === 1 ? label : undefined} />
            ))}
        </span>
    );
}

function WalletButton({ wallet, onConnect, label }: { wallet: UiWallet; onConnect: (a: UiWalletAccount) => void; label?: string }) {
    const [connecting, connect] = useConnect(wallet);
    return (
        <button type="button" className="wallet" disabled={connecting} onClick={() => connect().then(([a]) => a && onConnect(a))}>
            <img alt="" src={wallet.icon} width={18} height={18} /> {label ?? wallet.name}
        </button>
    );
}

function Home({ config, reports, member, open, choose }: { config: Config; reports: Report[]; member: boolean; open: (id: string) => void; choose: (p: Plan) => void }) {
    return (
        <main>
            <section className="hero">
                <p className="eyebrow">Weekly onchain research</p>
                <h1>See what moves before it moves.</h1>
                <p className="lede">One report a week on where payments, agents and markets are going on chain. Read by people who build there.</p>
                {member ? null : <a className="cta" href="#pricing">Become a member</a>}
            </section>

            <section id="research" className="reports">
                <h2>Latest reports</h2>
                {[...reports].reverse().map((r) => (
                    <button type="button" key={r.id} className="report" onClick={() => open(r.id)}>
                        <span className="meta">No. {r.issue} · {r.kicker} · {r.minutes} min</span>
                        <b>{r.title}</b>
                        <span className="dek">{r.dek}</span>
                        <span className={r.body ? 'tag open' : 'tag'}>{r.free ? 'Free to read' : r.body ? 'Member' : 'Members only'}</span>
                    </button>
                ))}
            </section>

            {member ? null : <Pricing plans={config.plans} choose={choose} />}
        </main>
    );
}

function Pricing({ plans, choose }: { plans: Plan[]; choose: (p: Plan) => void }) {
    return (
        <section id="pricing" className="pricing">
            <h2>Membership</h2>
            <p className="quiet">Pay in USDC from your own wallet. No card, no account, cancel in one tap.</p>
            <div className="plans">
                {plans.map((p) => (
                    <article key={p.id} className="plan">
                        <h3>{p.name}</h3>
                        <p className="price">{usd(p.price)}<span> / month</span></p>
                        <ul>{p.perks.map((perk) => <li key={perk}>{perk}</li>)}</ul>
                        <button type="button" className="cta" onClick={() => choose(p)}>Subscribe</button>
                    </article>
                ))}
            </div>
        </section>
    );
}

function ReportPage({ report, plans, choose, back }: { report?: Report; plans: Plan[]; choose: (p: Plan) => void; back: () => void }) {
    if (!report) return null;
    return (
        <main className="article">
            <button type="button" className="link" onClick={back}>← All reports</button>
            <p className="meta">No. {report.issue} · {report.kicker} · {report.minutes} min read</p>
            <h1>{report.title}</h1>
            <p className="lede">{report.dek}</p>
            {report.body ? (
                report.body.map((p) => <p key={p}>{p}</p>)
            ) : (
                <div className="paywall">
                    <h2>This report is for members</h2>
                    <p className="quiet">Subscribe from your wallet and read it now.</p>
                    <div className="row">
                        {plans.map((p) => (
                            <button type="button" key={p.id} className="cta" onClick={() => choose(p)}>{p.name} · {usd(p.price)} / month</button>
                        ))}
                    </div>
                </div>
            )}
        </main>
    );
}

/** Subscribe: one approval from the wallet, then Fathom collects the first month. */
function Checkout({ config, plan, onConnect, close, done }: { config: Config; plan: Plan; onConnect: (a: UiWalletAccount) => void; close: () => void; done: () => Promise<void> }) {
    const reader = useContext(ReaderContext);
    const [step, setStep] = useState('');
    const [error, setError] = useState('');

    const subscribe = async ({ account, me, signer }: Reader) => {
        setError('');
        try {
            const source = await ata(me);
            const token = await fetchMaybeToken(rpc, source);
            if (!token.exists || token.data.amount < BigInt(plan.price)) throw new Error(`Your wallet needs at least ${usd(plan.price)} in USDC`);
            const enabled = token.data.delegate.__option === 'Some' && token.data.delegate.value === ENGINE_ADDRESS;
            const enable = enabled ? [] : [getEnableInstruction({ account: source, owner: signer })];
            const terms = encode(subscriptionTerms({
                account: source,
                amount: BigInt(plan.price),
                merchant: config.merchant,
                mint: USDC,
                period: plan.period,
                start: await now(),
                subscriber: me,
            }));
            // One transaction: turn on USDC payments if needed, and approve the membership
            setStep('Approve the membership in your wallet…');
            await send(signer, [...enable, await getCreateInstruction({ authority: signer, payer: signer, terms })]);

            setStep('Collecting your first month…');
            await api('/subscribe', { address: me, plan: plan.id, terms: b64(terms) });
            await done();
        } catch (e) {
            setError(e instanceof Error ? e.message : String(e));
        }
        setStep('');
    };

    return (
        <div className="veil" onClick={(e) => e.target === e.currentTarget && !step && close()}>
            <section className="checkout" role="dialog" aria-label="Subscribe">
                <p className="eyebrow">Fathom {plan.name}</p>
                <h2>{usd(plan.price)} a month</h2>
                <ul className="terms">
                    <li>Charged today, then every 30 days, in USDC from your wallet.</li>
                    <li>Fathom can never take more than {usd(plan.price)} per period, and only into its own account.</li>
                    <li>Cancel from your account page or your wallet. It stops at once.</li>
                </ul>
                {error ? <p className="error">{error}</p> : null}
                {step ? <p className="step"><i className="spin" /> {step}</p> : null}
                {reader ? (
                    <button type="button" className="cta" disabled={!!step} onClick={() => subscribe(reader)}>Subscribe with {short(reader.me)}</button>
                ) : (
                    <>
                        <p className="quiet">Choose a wallet to pay with.</p>
                        <SignIn onConnect={onConnect} />
                    </>
                )}
                <button type="button" className="link" disabled={!!step} onClick={close}>Not now</button>
            </section>
        </div>
    );
}

function Account({ config, membership, plan, refresh, choose, signOut }: { config: Config; membership: Membership | null; plan?: Plan; refresh: () => Promise<void>; choose: (p: Plan) => void; signOut: () => void }) {
    const reader = useContext(ReaderContext);
    const [busy, setBusy] = useState(false);
    const [error, setError] = useState('');
    const [approval, setApproval] = useState('');
    useEffect(() => setApproval(membership ? text(unb64(membership.terms), { [USDC]: 6 }) : ''), [membership?.terms]);
    if (!reader) return null;

    if (!membership || !plan) {
        return (
            <main className="account">
                <h1>Your account</h1>
                <p className="quiet">You are signed in as {short(reader.me)} and have no membership yet.</p>
                <Pricing plans={config.plans} choose={choose} />
                <button type="button" className="link" onClick={signOut}>Sign out</button>
            </main>
        );
    }

    const cancel = async () => {
        setBusy(true);
        setError('');
        try {
            const terms = unb64(membership.terms);
            // Closing ends it at once, and the rent goes back to whoever paid it
            const policy = await fetchPolicy(rpc, terms, await now());
            await send(reader.signer, [getCloseInstruction({ account: await policyAddress(terms), closer: reader.signer, payer: policy?.payer ?? reader.me })]);
            await refresh();
        } catch (e) {
            setError(e instanceof Error ? e.message : String(e));
        }
        setBusy(false);
    };

    const access = membership.paidThrough > config.clock;
    const status = {
        active: { label: 'Active', line: `Renews ${day(membership.paidThrough)} for ${usd(plan.price)}` },
        cancelled: { label: 'Cancelled', line: access ? `You keep access until ${day(membership.paidThrough)}. Nothing more will be charged.` : 'Your membership has ended.' },
        past_due: { label: 'Payment failed', line: 'We could not collect this month. Add USDC to your wallet and we will try again.' },
    }[membership.status];

    return (
        <main className="account">
            <h1>Your account</h1>
            <section className="panel">
                <div className="split">
                    <div>
                        <p className="eyebrow">Fathom {plan.name}</p>
                        <h2>{usd(plan.price)} a month</h2>
                    </div>
                    <span className={`status ${membership.status}`}>{status.label}</span>
                </div>
                <p>{status.line}</p>
                <p className="quiet">Member since {day(membership.since)} · paying from {short(reader.me)}</p>
                {error ? <p className="error">{error}</p> : null}
                {membership.status === 'cancelled' ? (
                    <button type="button" className="cta" onClick={() => choose(plan)}>Subscribe again</button>
                ) : (
                    <button type="button" className="danger" disabled={busy} onClick={cancel}>{busy ? 'Cancelling…' : 'Cancel membership'}</button>
                )}
            </section>

            <section className="panel">
                <h3>Payments</h3>
                <table>
                    <tbody>
                        {membership.payments.map((p) => (
                            <tr key={p.signature}>
                                <td>{day(p.at)}</td>
                                <td>Fathom {plan.name}</td>
                                <td className="mono">{short(p.signature)}</td>
                                <td className="num">{usd(p.amount)}</td>
                            </tr>
                        ))}
                    </tbody>
                </table>
            </section>

            <section className="panel">
                <h3>What you approved</h3>
                <p className="quiet">
                    You approved this once, on chain. It is the only thing Fathom can do with your wallet, and the network enforces every line.
                </p>
                <pre>{approval}</pre>
            </section>
            <button type="button" className="link" onClick={signOut}>Sign out</button>
        </main>
    );
}

/** Dev only: test funds and the calendar of the local fork. */
function LocalFork({ refresh, clock }: { refresh: () => Promise<void>; clock: number }) {
    return (
        <span className="fork">
            Local fork · {day(clock)}
            <button type="button" onClick={() => api('/dev/skip', { days: 30 }).then(refresh).catch(console.error)}>Skip 30 days</button>
        </span>
    );
}
