import './styles.css';

import { loadWasm } from '@solana-pull/sdk';
import { createRoot } from 'react-dom/client';

import { Root } from './app.tsx';

await loadWasm();
// Dev builds: an in-page wallet, so the site runs without an extension
if (import.meta.env.DEV) await import('./wallet.ts');
createRoot(document.getElementById('root')!).render(<Root />);
