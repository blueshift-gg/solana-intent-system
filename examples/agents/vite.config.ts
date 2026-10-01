import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

import { paidApi } from './server.ts';

export default defineConfig({
    plugins: [react(), paidApi()],
    server: {
        port: 5173,
        fs: { allow: ['../..'] },
        // Surfpool, same origin, for the screen and for phones on the network
        proxy: { '/rpc': { target: 'http://127.0.0.1:8899', rewrite: () => '' } },
    },
});
