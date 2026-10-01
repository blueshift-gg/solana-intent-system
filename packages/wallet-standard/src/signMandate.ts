import type { SolanaSignOffchainMessageOutput } from '@solana/wallet-standard-features';
import type { ReadonlyUint8Array, WalletAccount } from '@wallet-standard/base';

/** Name of the feature. */
export const SolanaSignMandate = 'solana:signMandate';

/**
 * `solana:signMandate` is a feature that may be implemented by a {@link "@wallet-standard/base".Wallet}
 * to allow a dapp to request the wallet to sign Mandates: outcome-bound approvals the Mandate program
 * enforces on chain.
 *
 * The dapp sends the canonical terms, never text. The wallet decodes and validates them, reads each
 * mint's decimals from its own RPC, renders the canonical text itself and shows it, worst case first.
 * Terms with no expiry (`until revoked`) need a separate, explicit opt-in from the user, beyond the
 * ordinary confirmation. It then signs that text as an Offchain Message v1, so the output is exactly what
 * `solana:signOffchainMessage` would return for the same text, and what the program verifies.
 */
export type SolanaSignMandateFeature = {
    /** Name of the feature. */
    readonly [SolanaSignMandate]: {
        /** Version of the feature API. */
        readonly version: SolanaSignMandateVersion;

        /** Mandate terms format versions this wallet can decode, render and sign. */
        readonly supportedTermsVersions: readonly SolanaMandateTermsVersion[];

        /**
         * Sign Mandates using the account's secret key.
         *
         * @param inputs Mandates to sign.
         *
         * @return Results of signing. For each Mandate this includes the full Offchain Message v1
         * bytes the wallet rendered and signed, along with the resulting signature.
         */
        readonly signMandate: SolanaSignMandateMethod;
    };
};

/** Version of the feature. */
export type SolanaSignMandateVersion = '1.0.0';

/** Mandate terms format version (the first byte of the canonical terms). */
export type SolanaMandateTermsVersion = 1;

/**
 * Signs one or more Mandates, returning for each the full bytes the wallet constructed and signed
 * along with the resulting signature.
 *
 * @param inputs Mandates to sign.
 *
 * @return Results of signing Mandates.
 */
export type SolanaSignMandateMethod = (
    ...inputs: readonly SolanaSignMandateInput[]
) => Promise<readonly SolanaSignMandateOutput[]>;

/** Input for signing a Mandate. */
export interface SolanaSignMandateInput {
    /**
     * Account to use.
     * Must be the authority named in `terms`; the wallet rejects the request otherwise.
     */
    readonly account: WalletAccount;

    /**
     * Canonical Mandate terms. The wallet must reject bytes that are not a canonical encoding of
     * valid terms for the cluster the account is on.
     */
    readonly terms: ReadonlyUint8Array;
}

/**
 * Output of signing a Mandate: the Offchain Message v1 the wallet signed, with the canonical text
 * as its body, and the Ed25519 signature. `terms` plus `signature` is the portable approval
 * any executor can verify and fill.
 */
export type SolanaSignMandateOutput = SolanaSignOffchainMessageOutput;
