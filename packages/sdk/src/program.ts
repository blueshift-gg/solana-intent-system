import {
    AccountRole,
    address,
    type AccountMeta,
    type Address,
    getAddressEncoder,
    getProgramDerivedAddress,
    getI64Encoder,
    getU64Encoder,
    type Instruction,
    type TransactionSigner,
} from '@solana/kit';

export const MANDATE_PROGRAM_ADDRESS = address('Mand89p7P6okjEKQx2SpwDX6mdb5zAcdpshRafFtv7A');
/** The SPL delegate of every enabled token account, and the event signer. */
export const ENGINE_ADDRESS = address('6NpP2w9pBwSWBkQ7yNPYo5ruPY47goYB8DNsjBuKGyHp');
import { decode, termsId } from './terms.ts';

const SYSTEM = address('11111111111111111111111111111111');
const TOKEN_PROGRAM = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');

const key = (a: Address) => getAddressEncoder().encode(a);

export const findPolicyPda = async (authority: Address, id: Uint8Array) =>
    (await getProgramDerivedAddress({ programAddress: MANDATE_PROGRAM_ADDRESS, seeds: ['policy', key(authority), id] }))[0];

/** The page of nonces for intents of `authority` that expire at `notAfter`: one page per day of expiry. */
export const findNoncesPda = async (authority: Address, notAfter: number) =>
    (await getProgramDerivedAddress({ programAddress: MANDATE_PROGRAM_ADDRESS, seeds: ['nonces', key(authority), getI64Encoder().encode(Math.floor(notAfter / 86_400))] }))[0];

const signer = (s: TransactionSigner, role = AccountRole.READONLY_SIGNER): AccountMeta => ({ address: s.address, role, signer: s }) as AccountMeta;
const writable = (a: Address): AccountMeta => ({ address: a, role: AccountRole.WRITABLE });
const readonly = (a: Address): AccountMeta => ({ address: a, role: AccountRole.READONLY });
const tail = [readonly(ENGINE_ADDRESS), readonly(MANDATE_PROGRAM_ADDRESS)];

/** The address of the policy for canonical `terms`. */
export const policyAddress = async (terms: Uint8Array) => findPolicyPda(decode(terms).authority, await termsId(terms));

/**
 * Enable a token account: SPL `Approve` the engine as its delegate. `amount`
 * caps what every policy and intent on the account can pull in total; leave
 * it out for no cap beyond their own limits.
 */
export const getEnableInstruction = (p: { owner: TransactionSigner; account: Address; amount?: bigint; tokenProgram?: Address }): Instruction => ({
    accounts: [writable(p.account), readonly(ENGINE_ADDRESS), signer(p.owner)],
    data: Uint8Array.of(4, ...getU64Encoder().encode(p.amount ?? 2n ** 64n - 1n)),
    programAddress: p.tokenProgram ?? TOKEN_PROGRAM,
});

/**
 * Put a policy on chain: a standing permission its spender uses with pulls.
 * The authority signs; `payer` funds the rent and gets it back when the
 * policy closes.
 */
export const getCreateInstruction = async (p: { authority: TransactionSigner; payer: TransactionSigner; terms: Uint8Array }): Promise<Instruction> => ({
    accounts: [signer(p.authority), signer(p.payer, AccountRole.WRITABLE_SIGNER), writable(await policyAddress(p.terms)), readonly(SYSTEM), ...tail],
    data: Uint8Array.of(0, ...p.terms),
    programAddress: MANDATE_PROGRAM_ADDRESS,
});

type Legs = { terms: Uint8Array; to: Address; payFrom?: Address; tokenProgram?: Address; payTokenProgram?: Address };

/** The accounts a pull moves tokens between, shared by `Pull` and `Fill`. */
function legs(p: Legs & { from: Address }): AccountMeta[] {
    const t = decode(p.terms);
    const limit = t.limits.find((l) => l.from === p.from);
    if (!limit) throw new Error(`no limit of these terms covers ${p.from}`);
    if (t.price && !p.payFrom) throw new Error('these terms have a price: pass payFrom');
    const payment = t.price ? [writable(p.payFrom!), readonly(t.price.mint), writable(t.price.to), readonly(p.payTokenProgram ?? TOKEN_PROGRAM)] : [];
    return [writable(p.from), readonly(limit.mint), writable(p.to), ...tail, readonly(p.tokenProgram ?? TOKEN_PROGRAM), ...payment];
}

/**
 * Take `amount` under a policy, from `from` (a token account its terms
 * limit) into `to`. If the terms have a price, `payFrom` is the spender's
 * token account that pays it; the program moves the payment itself and
 * checks what arrives.
 */
export const getPullInstruction = async (p: Legs & { spender: TransactionSigner; from: Address; amount: bigint }): Promise<Instruction> => ({
    accounts: [signer(p.spender), writable(await policyAddress(p.terms)), ...legs(p)],
    data: Uint8Array.of(1, ...getU64Encoder().encode(p.amount)),
    programAddress: MANDATE_PROGRAM_ADDRESS,
});

/**
 * Run a signed intent, once: `terms` the authority signed as text
 * (`message()`), which must expire and limit one token account. Nothing goes
 * on chain first; `payer` funds the page of nonces if it is the first intent
 * of its expiry day, and gets that back when the day is over.
 */
export const getFillInstruction = async (
    p: Legs & { spender: TransactionSigner; payer: TransactionSigner; signature: Uint8Array; amount: bigint },
): Promise<Instruction> => {
    const t = decode(p.terms);
    if (t.notAfter === null) throw new Error('an intent must expire');
    return {
        accounts: [
            signer(p.spender),
            signer(p.payer, AccountRole.WRITABLE_SIGNER),
            writable(await findNoncesPda(t.authority, t.notAfter)),
            readonly(SYSTEM),
            ...legs({ ...p, from: t.limits[0].from }),
        ],
        data: Uint8Array.of(10, ...getU64Encoder().encode(p.amount), ...p.signature, ...p.terms),
        programAddress: MANDATE_PROGRAM_ADDRESS,
    };
};

/** Use up the nonce of a signed intent, so it can never be filled. */
export const getCancelInstruction = async (p: { authority: TransactionSigner; payer: TransactionSigner; terms: Uint8Array }): Promise<Instruction> => {
    const t = decode(p.terms);
    if (t.notAfter === null) throw new Error('an intent must expire');
    return {
        accounts: [
            signer(p.authority),
            signer(p.payer, AccountRole.WRITABLE_SIGNER),
            writable(await findNoncesPda(t.authority, t.notAfter)),
            readonly(SYSTEM),
            ...tail,
        ],
        data: Uint8Array.of(11, ...getI64Encoder().encode(t.notAfter), ...getU64Encoder().encode(BigInt(t.salt))),
        programAddress: MANDATE_PROGRAM_ADDRESS,
    };
};

/**
 * Close `account` and return its rent to `payer`, the account that paid it.
 * A policy (`policyAddress`): its authority or spender at any time, anyone
 * after its expiry. A page of nonces (`findNoncesPda`): anyone, once its day
 * is over.
 */
export const getCloseInstruction = (p: { closer: TransactionSigner; account: Address; payer: Address }): Instruction => ({
    accounts: [signer(p.closer), writable(p.account), writable(p.payer), ...tail],
    data: Uint8Array.of(2),
    programAddress: MANDATE_PROGRAM_ADDRESS,
});
