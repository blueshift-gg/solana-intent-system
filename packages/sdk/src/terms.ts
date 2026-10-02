import { type Address, getAddressEncoder } from '@solana/kit';

import init, { decodeTerms, encodeTerms, errorName, renderText } from '../wasm/mandate.js';

/**
 * Terms as JSON. Encoding, validation and the canonical text all
 * come from `mandate-core` compiled to WebAssembly, the same code the program
 * runs: this package never reimplements them. Integers that can pass 2^53 are
 * strings.
 */
export type Terms = {
    authority: Address;
    executor: Address | null;
    notBefore: number;
    notAfter: number | null;
    /** One execution only, whatever it takes. */
    once: boolean;
    /** The authority's current epoch (`fetchEpoch`). */
    epoch: string;
    /** Tells apart mandates whose terms are otherwise identical. */
    salt: string;
    takes: Take[];
    requires: Require[];
};

/**
 * At most `max` of `mint` may leave `from`, a token account of the authority.
 * `to` lists where a pull may go; empty means wherever the executor sends it.
 * Takes on one account stack: a pull must fit every one of them.
 */
export type Take = {
    from: Address;
    mint: Address;
    max: string;
    refill: 'never' | 'eachUse' | { over: { period: number } };
    to: Address[];
};

/** Token account `target` (of `mint`, owned by `owner`) must gain at least `bound`. */
export type Require = {
    target: Address;
    mint: Address;
    owner: Address;
    bound:
        | { const: string }
        | { linear: { t0: number; v0: string; t1: number; v1: string } }
        | { ratio: { of: number; num: string; den: string } };
};

let loaded: Promise<unknown> | undefined;

/** Load the WebAssembly once. Browsers fetch it; Node passes the bytes. */
export function loadMandate(module?: BufferSource) {
    loaded ??= init(module ? { module_or_path: module } : undefined);
    return loaded;
}

/** Canonical bytes: what a mandate holds and a signature covers. Throws on invalid terms. */
export const encode = (terms: Terms): Uint8Array => encodeTerms(JSON.stringify(terms));

export const decode = (bytes: Uint8Array): Terms => JSON.parse(decodeTerms(bytes));

/** The text a wallet shows. `decimals` maps each mint in the terms to its decimals. */
export const text = (bytes: Uint8Array, decimals: Record<string, number>): string =>
    renderText(bytes, JSON.stringify(decimals));

/** The name of a Mandate program error code (`Custom(code)` in a failed transaction). */
export const mandateError = (code: number): string | undefined => errorName(code);

/**
 * The exact bytes a wallet signs for signed terms: the Offchain Message v1
 * envelope (domain, version 1, one signer: the authority) and the canonical
 * text. `solana:signOffchainMessage` builds the same bytes from `text()`.
 */
export function message(bytes: Uint8Array, decimals: Record<string, number>): Uint8Array {
    const body = new TextEncoder().encode(text(bytes, decimals));
    const authority = getAddressEncoder().encode(decode(bytes).authority);
    return Uint8Array.from([0xff, ...new TextEncoder().encode('solana offchain'), 1, 1, ...authority, ...body]);
}

/** `mandate_id`: sha256 of the canonical bytes. */
export async function mandateId(bytes: Uint8Array): Promise<Uint8Array> {
    return new Uint8Array(await crypto.subtle.digest('SHA-256', bytes as BufferSource));
}

/** A fresh salt: the same terms can be approved again after a revocation. */
export const randomSalt = (): string => crypto.getRandomValues(new BigUint64Array(1))[0].toString();

/**
 * A subscription: at most `amount` from the subscriber's account, refilling
 * over `period`. Only the merchant may collect, and only into its own account.
 */
export function subscriptionTerms(p: {
    subscriber: Address;
    account: Address;
    mint: Address;
    amount: bigint;
    period: number;
    /** The key that collects: the only executor, and it may revoke. */
    merchant: Address;
    merchantAccount: Address;
    start: number;
    end?: number;
    /** The subscriber's current epoch (`fetchEpoch`). */
    epoch: string;
}): Terms {
    return {
        authority: p.subscriber,
        epoch: p.epoch,
        executor: p.merchant,
        notAfter: p.end ?? null,
        notBefore: p.start,
        once: false,
        requires: [],
        salt: randomSalt(),
        takes: [{ from: p.account, max: p.amount.toString(), mint: p.mint, refill: { over: { period: p.period } }, to: [p.merchantAccount] }],
    };
}

/** The limit of the first take of `terms`, as an amount. */
export const budget = (terms: Terms): bigint => BigInt(terms.takes[0].max);
