import {
    AccountRole,
    address,
    type AccountMeta,
    type Address,
    getAddressEncoder,
    getProgramDerivedAddress,
    getU16Encoder,
    getU64Encoder,
    type Instruction,
    type TransactionSigner,
} from '@solana/kit';

export const MANDATE_PROGRAM_ADDRESS = address('Mand89p7P6okjEKQx2SpwDX6mdb5zAcdpshRafFtv7A');
/** The SPL delegate of every enabled token account, and the event signer. */
export const ENGINE_ADDRESS = address('6NpP2w9pBwSWBkQ7yNPYo5ruPY47goYB8DNsjBuKGyHp');
import { decode, mandateId } from './terms.ts';

const SYSTEM = address('11111111111111111111111111111111');
const TOKEN_PROGRAM = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');

const key = (a: Address) => getAddressEncoder().encode(a);

export const findMandatePda = async (authority: Address, id: Uint8Array) =>
    (await getProgramDerivedAddress({ programAddress: MANDATE_PROGRAM_ADDRESS, seeds: ['mandate', key(authority), id] }))[0];

const signer = (s: TransactionSigner, role = AccountRole.READONLY_SIGNER): AccountMeta => ({ address: s.address, role, signer: s }) as AccountMeta;
const writable = (a: Address): AccountMeta => ({ address: a, role: AccountRole.WRITABLE });
const readonly = (a: Address): AccountMeta => ({ address: a, role: AccountRole.READONLY });
const tail = [readonly(ENGINE_ADDRESS), readonly(MANDATE_PROGRAM_ADDRESS)];

const mandateOf = async (terms: Uint8Array) => findMandatePda(decode(terms).authority, await mandateId(terms));

/**
 * Enable a token account: SPL `Approve` the engine as its delegate. `amount`
 * caps what every mandate on the account can pull in total; leave it out for
 * no cap beyond each mandate's own limits.
 */
export const getEnableInstruction = (p: { owner: TransactionSigner; account: Address; amount?: bigint; tokenProgram?: Address }): Instruction => ({
    accounts: [writable(p.account), readonly(ENGINE_ADDRESS), signer(p.owner)],
    data: Uint8Array.of(4, ...getU64Encoder().encode(p.amount ?? 2n ** 64n - 1n)),
    programAddress: p.tokenProgram ?? TOKEN_PROGRAM,
});

/**
 * Put a mandate on chain. The authority signs this transaction, or anyone
 * brings the authority's `signature` over the canonical text along with the
 * `mints` the terms name: then the authority signs nothing here. Signed terms
 * must expire. `payer` funds the rent and gets it back when the mandate closes.
 */
export const getCreateInstruction = async (
    p: { payer: TransactionSigner; terms: Uint8Array } & ({ authority: TransactionSigner } | { signature: Uint8Array; mints: Address[] }),
): Promise<Instruction> => {
    const signed = 'signature' in p;
    return {
        accounts: [
            signed ? readonly(decode(p.terms).authority) : signer(p.authority),
            signer(p.payer, AccountRole.WRITABLE_SIGNER),
            writable(await mandateOf(p.terms)),
            readonly(SYSTEM),
            ...tail,
            ...(signed ? p.mints.map(readonly) : []),
        ],
        data: Uint8Array.of(0, ...getU16Encoder().encode(p.terms.length), ...p.terms, ...(signed ? p.signature : [])),
        programAddress: MANDATE_PROGRAM_ADDRESS,
    };
};

/**
 * Take `amount` from `from` (a token account the terms limit) into `to`. If
 * the terms have a price, `payFrom` is the spender's token account that pays
 * it; the program moves the payment itself and checks what arrives.
 */
export const getPullInstruction = async (p: {
    spender: TransactionSigner;
    terms: Uint8Array;
    from: Address;
    to: Address;
    amount: bigint;
    payFrom?: Address;
    tokenProgram?: Address;
    payTokenProgram?: Address;
}): Promise<Instruction> => {
    const t = decode(p.terms);
    const limit = t.limits.find((l) => l.from === p.from);
    if (!limit) throw new Error(`no limit of these terms covers ${p.from}`);
    if (t.price && !p.payFrom) throw new Error('these terms have a price: pass payFrom');
    const payment = t.price ? [writable(p.payFrom!), readonly(t.price.mint), writable(t.price.to), readonly(p.payTokenProgram ?? TOKEN_PROGRAM)] : [];
    return {
        accounts: [
            signer(p.spender),
            writable(await mandateOf(p.terms)),
            writable(p.from),
            readonly(limit.mint),
            writable(p.to),
            ...tail,
            readonly(p.tokenProgram ?? TOKEN_PROGRAM),
            ...payment,
        ],
        data: Uint8Array.of(1, ...getU64Encoder().encode(p.amount)),
        programAddress: MANDATE_PROGRAM_ADDRESS,
    };
};

/**
 * End a mandate. The authority or the spender may at any time; anyone may
 * after its expiry. A mandate that never expires closes at once and `payer`
 * (the account that paid its rent, see `fetchMandate`) is refunded. One that
 * expires is only marked revoked until then; close it again afterwards.
 */
export const getCloseInstruction = async (p: { closer: TransactionSigner; terms: Uint8Array; payer: Address }): Promise<Instruction> => ({
    accounts: [signer(p.closer), writable(await mandateOf(p.terms)), writable(p.payer), ...tail],
    data: Uint8Array.of(2),
    programAddress: MANDATE_PROGRAM_ADDRESS,
});
