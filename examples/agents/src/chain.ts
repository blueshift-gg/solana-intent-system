import { type Address, address, appendTransactionMessageInstructions, createSolanaRpc, createTransactionMessage, getBase64EncodedWireTransaction, type Instruction, pipe, setTransactionMessageFeePayerSigner, setTransactionMessageLifetimeUsingBlockhash, signTransactionMessageWithSigners, type TransactionSigner } from '@solana/kit';
import { fetchSysvarClock } from '@solana/sysvars';
import { fetchMaybeToken, findAssociatedTokenPda, TOKEN_PROGRAM_ADDRESS } from '@solana-program/token';

export const rpc = createSolanaRpc(`${location.origin}/rpc`);
export const USDC = address('EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v');
export const DAY = 86_400;

/** What the server shares: the API, and what the owner's phone approved. */
export type Shared = {
    alice?: Address;
    budget?: { terms: string };
    perDay: string;
    price: string;
    provider: Address;
    providerUsdc: Address;
    mallory: Address;
    phoneUrl: string;
    clock: number;
};

export const api = async <T = unknown>(path: string, body?: unknown): Promise<T> => {
    const res = await fetch(`/api${path}`, body === undefined ? undefined : { body: JSON.stringify(body), method: 'POST' });
    return res.json();
};

export const now = async () => Number((await fetchSysvarClock(rpc)).unixTimestamp);
export const usdcAccount = async (owner: Address) => (await findAssociatedTokenPda({ mint: USDC, owner, tokenProgram: TOKEN_PROGRAM_ADDRESS }))[0];

export async function usdcOf(owner: Address) {
    const token = await fetchMaybeToken(rpc, await usdcAccount(owner));
    return token.exists ? token.data : null;
}

export const usd = (raw: bigint | number | string) => `$${(Number(raw) / 1e6).toLocaleString('en-US', { maximumFractionDigits: 2, minimumFractionDigits: 2 })}`;
export const b64 = (bytes: Uint8Array) => btoa(String.fromCharCode(...bytes));
export const unb64 = (s: string) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));

let lastBlockhash = '';

/** Sign and send; a repeated action waits for a fresh blockhash so it is a new transaction. */
export async function send(payer: TransactionSigner, instructions: Instruction[]) {
    let { value: blockhash } = await rpc.getLatestBlockhash().send();
    while (blockhash.blockhash === lastBlockhash) {
        await new Promise((r) => setTimeout(r, 80));
        ({ value: blockhash } = await rpc.getLatestBlockhash().send());
    }
    lastBlockhash = blockhash.blockhash;
    const tx = await signTransactionMessageWithSigners(
        pipe(
            createTransactionMessage({ version: 0 }),
            (m) => setTransactionMessageFeePayerSigner(payer, m),
            (m) => setTransactionMessageLifetimeUsingBlockhash(blockhash, m),
            (m) => appendTransactionMessageInstructions(instructions, m),
        ),
    );
    const signature = await rpc.sendTransaction(getBase64EncodedWireTransaction(tx), { encoding: 'base64' }).send();
    for (let i = 0; i < 60; i++) {
        const [status] = (await rpc.getSignatureStatuses([signature]).send()).value;
        if (status?.confirmationStatus) return signature;
        await new Promise((r) => setTimeout(r, 150));
    }
    throw new Error('Not confirmed');
}

/** Poll `fn` every `ms` while mounted. */
export function every(ms: number, fn: () => Promise<void>) {
    let live = true;
    const tick = async () => {
        await fn().catch(console.error);
        if (live) setTimeout(tick, ms);
    };
    tick();
    return () => {
        live = false;
    };
}
