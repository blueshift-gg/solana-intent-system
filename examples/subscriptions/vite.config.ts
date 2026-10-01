import react from '@vitejs/plugin-react';
import { defineConfig } from 'vite';

import { fathom } from './server.ts';

export default defineConfig({
    plugins: [react(), fathom()],
    server: {
        port: 5173,
        fs: { allow: ['../..'] },
        // The cluster, same origin: no CORS, and the page never learns the port
        proxy: { '/rpc': { target: 'http://127.0.0.1:8899', rewrite: () => '' } },
    },
});
