// npm run dev [app]: build the program and the SDK's WebAssembly, start
// Surfpool forking mainnet with the program installed at its address by the
// surfnet-setup runbook, then serve an example (default: the agents demo).
import { execSync, spawn } from 'node:child_process';

const PROGRAM = 'PULLrgDYqK1yFKVTSbWieX3ARP7U2XUyrjxWXqKgVzA';
const APP = process.argv[2] ?? 'agents';
const children = [];

execSync('cargo build-sbf --manifest-path program/Cargo.toml --features localnet', { stdio: 'inherit' });
execSync('npm run build -w @solana-pull/sdk', { stdio: 'inherit' });

function start(command, args, stdio = 'inherit') {
    const child = spawn(command, args, { stdio });
    child.on('exit', (code) => stop(code ?? 0));
    children.push(child);
}

function stop(code) {
    children.forEach((c) => c.exitCode === null && c.kill());
    process.exit(code);
}
process.on('SIGINT', () => stop(0));
process.on('SIGTERM', () => stop(0));

// Surfpool logs to .surfpool/logs; Studio shows every transaction
start('surfpool', ['start', '--no-tui', '--yes', '--runbook', 'surfnet-setup'], 'ignore');

const executable = async () => {
    const body = JSON.stringify({ id: 1, jsonrpc: '2.0', method: 'getAccountInfo', params: [PROGRAM, { encoding: 'base64' }] });
    const res = await fetch('http://127.0.0.1:8899', { body, headers: { 'content-type': 'application/json' }, method: 'POST' });
    return (await res.json()).result?.value?.executable === true;
};
for (let i = 0; !(await executable().catch(() => false)); i++) {
    if (i === 60) throw new Error('Surfpool did not install the program within 30 s; see .surfpool/logs');
    await new Promise((r) => setTimeout(r, 500));
}
console.log('\nSurfpool: mainnet fork on http://127.0.0.1:8899, Studio on http://127.0.0.1:18488');
console.log(`Program:  ${PROGRAM}\n`);

start('npm', ['run', 'dev', '-w', APP]);
