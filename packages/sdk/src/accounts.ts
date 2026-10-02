import { getAddressDecoder, getBase64Encoder, type Rpc, type SolanaRpcApi } from '@solana/kit';

import { findMandatePda } from './program.ts';
import { decode, mandateId } from './terms.ts';

/**
 * A mandate's account: null until it is created, by its authority or by
 * whoever brings its signature. `spent` is what each limit has consumed at
 * `now` (pass the Clock sysvar's time): a periodic limit starts every window
 * at zero, as in the program.
 */
export async function fetchMandate(rpc: Rpc<SolanaRpcApi>, terms: Uint8Array, now: number) {
    const t = decode(terms);
    const { value } = await rpc.getAccountInfo(await findMandatePda(t.authority, await mandateId(terms)), { encoding: 'base64' }).send();
    if (!value) return null;
    const data = new Uint8Array(getBase64Encoder().encode(value.data[0]));
    const view = new DataView(data.buffer, data.byteOffset);
    // tag, flags, rolled: i64, consumed: [u64; 8], payer
    const rolled = Number(view.getBigInt64(2, true));
    const spent = t.limits.map((limit, k) => {
        const consumed = view.getBigUint64(10 + 8 * k, true);
        if (limit.per === 'total') return consumed;
        if (limit.per === 'use') return 0n;
        const window = (at: number) => Math.floor((at - t.notBefore) / (limit.per as { every: number }).every);
        return window(now) === window(rolled) ? consumed : 0n;
    });
    return { payer: getAddressDecoder().decode(data.subarray(74, 106)), revoked: (data[1] & 1) === 1, spent };
}
