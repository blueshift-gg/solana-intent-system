import { getAddressDecoder, getBase64Encoder, type Rpc, type SolanaRpcApi } from '@solana/kit';

import { findPolicyPda, findNoncesPda } from './program.ts';
import { decode, termsId } from './terms.ts';

const NONCE_BITS = 1024n;

const read = async (rpc: Rpc<SolanaRpcApi>, account: Parameters<Rpc<SolanaRpcApi>['getAccountInfo']>[0]) => {
    const { value } = await rpc.getAccountInfo(account, { encoding: 'base64' }).send();
    return value ? new Uint8Array(getBase64Encoder().encode(value.data[0])) : null;
};

/**
 * A mandate's account: null if it is not on chain (never created, or closed).
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

/** Whether a signed intent's nonce is used: it ran, or its authority cancelled it. */
export async function fetchIntentUsed(rpc: Rpc<SolanaRpcApi>, terms: Uint8Array): Promise<boolean> {
    const t = decode(terms);
    if (t.notAfter === null) throw new Error('an intent must expire');
    const data = await read(rpc, await findNoncesPda(t.authority, t.notAfter));
    if (!data) return false;
    // tag, payer, day: i64, bits
    const bit = Number(BigInt(t.salt) % NONCE_BITS);
    return (data[41 + (bit >> 3)] & (1 << (bit & 7))) !== 0;
}
