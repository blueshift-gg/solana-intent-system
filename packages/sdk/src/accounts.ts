import { type Address, getAddressDecoder, getBase64Encoder, type Rpc, type SolanaRpcApi } from '@solana/kit';

import { findNoncesPda, findPolicyPda, NONCE_BITS } from './program.ts';
import { decode, termsId } from './terms.ts';

const read = async (rpc: Rpc<SolanaRpcApi>, account: Parameters<Rpc<SolanaRpcApi>['getAccountInfo']>[0]) => {
    const { value } = await rpc.getAccountInfo(account, { encoding: 'base64' }).send();
    return value ? new Uint8Array(getBase64Encoder().encode(value.data[0])) : null;
};

/**
 * A policy's account: null if it is not on chain (never created, or closed).
 * `spent` is what each limit has consumed at `now` (pass the Clock sysvar's
 * time): a periodic limit starts every window at zero, as in the program.
 */
export async function fetchPolicy(rpc: Rpc<SolanaRpcApi>, terms: Uint8Array, now: number) {
    const t = decode(terms);
    const data = await read(rpc, await findPolicyPda(t.authority, await termsId(terms)));
    if (!data) return null;
    const view = new DataView(data.buffer, data.byteOffset);
    // tag, rolled: i64, consumed: [u64; 8], payer
    const rolled = Number(view.getBigInt64(1, true));
    const spent = t.limits.map((limit, k) => {
        const consumed = view.getBigUint64(9 + 8 * k, true);
        if (limit.per === 'total') return consumed;
        if (limit.per === 'use') return 0n;
        const window = (at: number) => Math.floor((at - t.notBefore) / (limit.per as { every: number }).every);
        return window(now) === window(rolled) ? consumed : 0n;
    });
    return { payer: getAddressDecoder().decode(data.subarray(73, 105)), spent };
}

/** The used-nonce bits of one page, or null if the page is not on chain. */
async function fetchNonces(rpc: Rpc<SolanaRpcApi>, authority: Address, notAfter: number, salt: bigint) {
    // tag, payer, authority, day: i64, page: u64, bits
    return (await read(rpc, await findNoncesPda(authority, notAfter, salt)))?.subarray(81) ?? null;
}

const used = (bits: Uint8Array | null, salt: bigint) => {
    const bit = Number(salt % NONCE_BITS);
    return !!bits && (bits[bit >> 3] & (1 << (bit & 7))) !== 0;
};

/** Whether a signed intent's nonce is used: it ran, or its authority cancelled it. */
export async function fetchIntentUsed(rpc: Rpc<SolanaRpcApi>, terms: Uint8Array): Promise<boolean> {
    const t = decode(terms);
    if (t.notAfter === null) throw new Error('an intent must expire');
    return used(await fetchNonces(rpc, t.authority, t.notAfter, BigInt(t.salt)), BigInt(t.salt));
}

/**
 * The lowest salt whose nonce is free for an intent of `authority` that
 * expires at `notAfter`. Salts handed out this way share pages, so an owner
 * pays for one page per 1,024 intents that expire on the same day. `skip`
 * are salts already given to intents that are signed but not yet on chain.
 */
export async function nextSalt(rpc: Rpc<SolanaRpcApi>, authority: Address, notAfter: number, skip: string[] = []): Promise<string> {
    for (let page = 0n; ; page++) {
        const bits = await fetchNonces(rpc, authority, notAfter, page * NONCE_BITS);
        for (let salt = page * NONCE_BITS; salt < (page + 1n) * NONCE_BITS; salt++) {
            if (!used(bits, salt) && !skip.includes(salt.toString())) return salt.toString();
        }
    }
}
