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
const INSTRUCTIONS_SYSVAR = address('Sysvar1nstructions1111111111111111111111111');

const key = (a: Address) => getAddressEncoder().encode(a);
const pda = async (seeds: Parameters<typeof getProgramDerivedAddress>[0]['seeds']) =>
    (await getProgramDerivedAddress({ programAddress: MANDATE_PROGRAM_ADDRESS, seeds }))[0];

export const findMandatePda = (authority: Address, id: Uint8Array) => pda(['mandate', key(authority), id]);
export const findSessionPda = (executor: Address) => pda(['session', key(executor)]);
export const findEpochPda = (authority: Address) => pda(['epoch', key(authority)]);

const signer = (s: TransactionSigner, role = AccountRole.READONLY_SIGNER): AccountMeta => ({ address: s.address, role, signer: s }) as AccountMeta;
const payer = (s: TransactionSigner) => signer(s, AccountRole.WRITABLE_SIGNER);
const writable = (a: Address): AccountMeta => ({ address: a, role: AccountRole.WRITABLE });
const readonly = (a: Address): AccountMeta => ({ address: a, role: AccountRole.READONLY });
const tail = [readonly(ENGINE_ADDRESS), readonly(MANDATE_PROGRAM_ADDRESS)];

/** The mandate account and the authority's Epoch account for canonical `terms`. */
const accountsOf = async (terms: Uint8Array) => {
    const { authority } = decode(terms);
    return { authority, epoch: await findEpochPda(authority), mandate: await findMandatePda(authority, await mandateId(terms)) };
};

/**
 * Put a mandate on chain. The authority signs this transaction, or anyone
 * brings the authority's `signature` over the canonical text along with the
 * `mints` the terms name: then the authority signs nothing here and `payer`
 * funds the rent.
 */
export const getCreateMandateInstruction = async (
    p: { payer: TransactionSigner; terms: Uint8Array } & ({ authority: TransactionSigner } | { signature: Uint8Array; mints: Address[] }),
): Promise<Instruction> => {
    const { authority, epoch, mandate } = await accountsOf(p.terms);
    const signed = 'signature' in p;
    return {
        accounts: [
            signed ? readonly(authority) : signer(p.authority),
            payer(p.payer),
            writable(mandate),
            readonly(epoch),
            readonly(SYSTEM),
            ...tail,
            ...(signed ? p.mints.map(readonly) : []),
        ],
        data: Uint8Array.of(0, ...getU16Encoder().encode(p.terms.length), ...p.terms, ...(signed ? p.signature : [])),
        programAddress: MANDATE_PROGRAM_ADDRESS,
    };
};

/**
 * Revoke one mandate. `revoker` is its authority (the mandate may be on chain
 * or only signed) or the executor its terms name (once it is on chain).
 */
export const getRevokeMandateInstruction = async (p: { revoker: TransactionSigner; payer: TransactionSigner; terms: Uint8Array }): Promise<Instruction> => {
    const { epoch, mandate } = await accountsOf(p.terms);
    return {
        accounts: [signer(p.revoker), payer(p.payer), writable(mandate), readonly(epoch), readonly(SYSTEM), ...tail],
        data: Uint8Array.of(1, ...(await mandateId(p.terms))),
        programAddress: MANDATE_PROGRAM_ADDRESS,
    };
};

/** Revoke every mandate of the authority, created or only signed. */
export const getBumpEpochInstruction = async (p: { authority: TransactionSigner; payer: TransactionSigner }): Promise<Instruction> => ({
    accounts: [signer(p.authority), payer(p.payer), writable(await findEpochPda(p.authority.address)), readonly(SYSTEM), ...tail],
    data: Uint8Array.of(2),
    programAddress: MANDATE_PROGRAM_ADDRESS,
});

const TOKEN_PROGRAM = address('TokenkegQfeZyiNwAJbNbGKPFXCWuBvf9Ss623VQ5DA');

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

export type Pull = { from: Address; to: Address; amount: bigint };

/**
 * Execute a mandate that is on chain. `accounts` are the token accounts, mints
 * and token program the pulls touch. A payment (terms without requirements)
 * is this one instruction; an exchange joins the transaction's session and
 * needs a `Close` after it, and `payer` funds that session on first use.
 */
export const getOpenInstruction = async (p: {
    executor: TransactionSigner;
    payer: TransactionSigner;
    terms: Uint8Array;
    accounts: AccountMeta[];
    pulls: Pull[];
}): Promise<Instruction> => {
    const index = (a: Address) => {
        const i = p.accounts.findIndex((m) => m.address === a);
        if (i < 0) throw new Error(`pull account ${a} is not in accounts`);
        return i;
    };
    const pulls = p.pulls.flatMap(({ from, to, amount }) => [index(from), index(to), ...getU64Encoder().encode(amount)]);
    const { epoch, mandate } = await accountsOf(p.terms);
    return {
        accounts: [
            signer(p.executor),
            writable(await findSessionPda(p.executor.address)),
            payer(p.payer),
            writable(mandate),
            readonly(epoch),
            readonly(INSTRUCTIONS_SYSVAR),
            readonly(SYSTEM),
            ...tail,
            ...p.accounts,
        ],
        data: Uint8Array.of(20, p.pulls.length, ...pulls),
        programAddress: MANDATE_PROGRAM_ADDRESS,
    };
};

/** Check every outcome of the session's mandates; `targets` are the accounts they name. */
export const getCloseInstruction = async (p: { executor: TransactionSigner; targets: Address[] }): Promise<Instruction> => ({
    accounts: [signer(p.executor), writable(await findSessionPda(p.executor.address)), ...tail, ...p.targets.map(readonly)],
    data: Uint8Array.of(21),
    programAddress: MANDATE_PROGRAM_ADDRESS,
});

/**
 * Close a mandate that has expired or whose epoch has moved, and return its rent to `payer`,
 * the account that paid it (`fetchMandate`). Anyone may send this.
 */
export const getCloseMandateInstruction = async (p: { terms: Uint8Array; payer: Address }): Promise<Instruction> => {
    const { epoch, mandate } = await accountsOf(p.terms);
    return {
        accounts: [writable(mandate), writable(p.payer), readonly(epoch), ...tail],
        data: Uint8Array.of(22),
        programAddress: MANDATE_PROGRAM_ADDRESS,
    };
};
