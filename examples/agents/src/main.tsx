import './styles.css';

import { loadMandate } from '@mandate/sdk';
import { createRoot } from 'react-dom/client';

import { Phone } from './phone.tsx';
import { Screen } from './screen.tsx';

await loadMandate();
createRoot(document.getElementById('root')!).render(location.pathname.startsWith('/phone') ? <Phone /> : <Screen />);
