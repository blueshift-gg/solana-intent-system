import {
    type Address,
    address,
    appendTransactionMessageInstructions,
    createSolanaRpc,
    createTransactionMessage,
    getBase64EncodedWireTransaction,
    type Instruction,
    pipe,
    setTransactionMessageFeePayerSigner,
    setTransactionMessageLifetimeUsingBlockhash,
    signTransactionMessageWithSigners,
    type TransactionSigner,
} from '@solana/kit';
import { fetchSysvarClock } from '@solana/sysvars';
import { findAssociatedTokenPda, TOKEN_PROGRAM_ADDRESS } from '@solana-program/token';

/** The cluster, behind the dev server's /rpc proxy. */
export const rpc = createSolanaRpc(`${location.origin}/rpc`);
export const USDC = address('EPjFWdd5AufqSSqeM2qN1xzybapC8G4wEGGkZwyTDt1v');
export const DAY = 86_400;

export const ata = async (owner: Address) =>
    (await findAssociatedTokenPda({ mint: USDC, owner, tokenProgram: TOKEN_PROGRAM_ADDRESS }))[0];

/** The Clock sysvar: the time the program checks. */
export const now = async () => Number((await fetchSysvarClock(rpc)).unixTimestamp);

/** Fathom's backend. A refusal throws its reason. */
export async function api<T = unknown>(path: string, body?: unknown): Promise<T> {
    const res = await fetch(`/api${path}`, body === undefined ? undefined : { body: JSON.stringify(body), method: 'POST' });
    const json = await res.json();
    if (!res.ok) throw new Error(json.error ?? 'Something went wrong');
    return json;
}

/** Sign with the wallet, send, and wait. */
export async function send(payer: TransactionSigner, instructions: Instruction[]) {
    const { value: blockhash } = await rpc.getLatestBlockhash().send();
    const message = pipe(
        createTransactionMessage({ version: 0 }),
        (m) => setTransactionMessageFeePayerSigner(payer, m),
        (m) => setTransactionMessageLifetimeUsingBlockhash(blockhash, m),
        (m) => appendTransactionMessageInstructions(instructions, m),
    );
    const tx = await signTransactionMessageWithSigners(message);
    const signature = await rpc.sendTransaction(getBase64EncodedWireTransaction(tx), { encoding: 'base64' }).send();
    for (let i = 0; i < 60; i++) {
        const [status] = (await rpc.getSignatureStatuses([signature]).send()).value;
        if (status?.confirmationStatus) {
            if (status.err) throw new Error('The transaction failed');
            return signature;
        }
        await new Promise((r) => setTimeout(r, 250));
    }
    throw new Error('Not confirmed after 15 seconds');
}

export const usd = (raw: bigint | number | string) => {
    const n = Number(raw) / 1e6;
    return `$${n.toLocaleString('en-US', { maximumFractionDigits: 2, minimumFractionDigits: Number.isInteger(n) ? 0 : 2 })}`;
};
export const day = (unix: number) => new Date(unix * 1000).toLocaleDateString('en-US', { day: 'numeric', month: 'short', year: 'numeric' });
export const short = (a: string) => `${a.slice(0, 4)}…${a.slice(-4)}`;
export const b64 = (bytes: Uint8Array) => btoa(String.fromCharCode(...bytes));
export const unb64 = (s: string) => Uint8Array.from(atob(s), (c) => c.charCodeAt(0));
