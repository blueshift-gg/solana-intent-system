import { type Address, getAddressDecoder, getBase64Encoder, type Rpc, type SolanaRpcApi } from '@solana/kit';

import { findEpochPda, findMandatePda } from './program.ts';
import { decode, mandateId } from './terms.ts';

/** What a refilling take has consumed at `now`: the budget refills linearly over the period, as in the program. */
export function spentAt(ledger: { consumed: bigint; rolled: number }, period: number, now: number, budget: bigint): bigint {
    const elapsed = Math.min(Math.max(now - ledger.rolled, 0), period);
    const back = (budget * BigInt(elapsed)) / BigInt(period);
    return ledger.consumed > back ? ledger.consumed - back : 0n;
}

/** The authority's current epoch: new terms must carry it. 0 until the first BumpEpoch. */
export async function fetchEpoch(rpc: Rpc<SolanaRpcApi>, authority: Address): Promise<number> {
    const { value } = await rpc.getAccountInfo(await findEpochPda(authority), { encoding: 'base64' }).send();
    if (!value) return 0;
    const data = new Uint8Array(getBase64Encoder().encode(value.data[0]));
    return new DataView(data.buffer, data.byteOffset).getUint32(33, true); // tag, authority, epoch
}

/**
 * A mandate's account: null until it is created, by its authority or by the
 * first executor that brings its signature. `consumed` is the first take's.
 */
export async function fetchMandate(rpc: Rpc<SolanaRpcApi>, terms: Uint8Array) {
    const t = decode(terms);
    const { value } = await rpc.getAccountInfo(await findMandatePda(t.authority, await mandateId(terms)), { encoding: 'base64' }).send();
    if (!value) return null;
    const data = new Uint8Array(getBase64Encoder().encode(value.data[0]));
    const view = new DataView(data.buffer, data.byteOffset);
    // tag, flags, rolled, consumed[8], not_after, epoch, authority, payer
    return {
        consumed: view.getBigUint64(10, true),
        done: (data[1] & 1) === 1,
        payer: getAddressDecoder().decode(data.subarray(118, 150)),
        revoked: (data[1] & 2) === 2,
        rolled: Number(view.getBigInt64(2, true)),
    };
}
