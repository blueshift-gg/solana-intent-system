import './styles.css';

import { loadWasm } from '@solana-pull/sdk';
import { createRoot } from 'react-dom/client';

import { Phone } from './phone.tsx';
import { Screen } from './screen.tsx';

await loadWasm();
createRoot(document.getElementById('root')!).render(location.pathname.startsWith('/phone') ? <Phone /> : <Screen />);
