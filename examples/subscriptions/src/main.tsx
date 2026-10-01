import './styles.css';

import { loadMandate } from '@mandate/sdk';
import { createRoot } from 'react-dom/client';

import { Root } from './app.tsx';

await loadMandate();
// Dev builds: an in-page wallet, so the site runs without an extension
if (import.meta.env.DEV) await import('./wallet.ts');
createRoot(document.getElementById('root')!).render(<Root />);
