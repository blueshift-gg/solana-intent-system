import type { SolanaSignOffchainMessageOutput } from '@solana/wallet-standard-features';
import type { WalletAccount, WalletWithFeatures } from '@wallet-standard/base';

import type {
    SolanaSignMandateFeature,
    SolanaSignMandateInput,
    SolanaSignMandateOutput,
} from '../signMandate.js';
import { SolanaSignMandate } from '../signMandate.js';

const account = null as unknown as WalletAccount;

// [DESCRIBE] `SolanaSignMandateInput`
{
    // The account and the canonical terms are all a dapp sends: no text.
    {
        ({ account, terms: new Uint8Array() }) satisfies SolanaSignMandateInput;
    }

    // Text is not an input.
    {
        // @ts-expect-error The wallet renders the text itself.
        ({ account, terms: new Uint8Array(), message: '' }) satisfies SolanaSignMandateInput;
    }
}

// [DESCRIBE] `SolanaSignMandateOutput`
{
    // It is exactly the Offchain Message output, so verifiers need nothing new.
    {
        const output = null as unknown as SolanaSignMandateOutput;
        output satisfies SolanaSignOffchainMessageOutput;
    }
}

// [DESCRIBE] Feature detection
{
    // A dapp narrows a wallet to one that supports the feature, then calls it.
    {
        const wallet = null as unknown as WalletWithFeatures<SolanaSignMandateFeature>;
        const feature = wallet.features[SolanaSignMandate];
        feature.supportedTermsVersions satisfies readonly 1[];
        void feature.signMandate({ account, terms: new Uint8Array() });
    }
}
