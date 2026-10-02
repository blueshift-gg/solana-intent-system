import type { SolanaSignOffchainMessageOutput } from '@solana/wallet-standard-features';
import type { WalletAccount, WalletWithFeatures } from '@wallet-standard/base';

import type {
    SolanaSignIntentFeature,
    SolanaSignIntentInput,
    SolanaSignIntentOutput,
} from '../signIntent.js';
import { SolanaSignIntent } from '../signIntent.js';

const account = null as unknown as WalletAccount;

// [DESCRIBE] `SolanaSignIntentInput`
{
    // The account and the canonical terms are all a dapp sends: no text.
    {
        ({ account, terms: new Uint8Array() }) satisfies SolanaSignIntentInput;
    }

    // Text is not an input.
    {
        // @ts-expect-error The wallet renders the text itself.
        ({ account, terms: new Uint8Array(), message: '' }) satisfies SolanaSignIntentInput;
    }
}

// [DESCRIBE] `SolanaSignIntentOutput`
{
    // It is exactly the Offchain Message output, so verifiers need nothing new.
    {
        const output = null as unknown as SolanaSignIntentOutput;
        output satisfies SolanaSignOffchainMessageOutput;
    }
}

// [DESCRIBE] Feature detection
{
    // A dapp narrows a wallet to one that supports the feature, then calls it.
    {
        const wallet = null as unknown as WalletWithFeatures<SolanaSignIntentFeature>;
        const feature = wallet.features[SolanaSignIntent];
        feature.supportedTermsVersions satisfies readonly 1[];
        void feature.signIntent({ account, terms: new Uint8Array() });
    }
}
