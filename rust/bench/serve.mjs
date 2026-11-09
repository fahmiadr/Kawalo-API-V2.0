#!/usr/bin/env node
// Starts/stops a server under test with the shared bench environment and records pid + time-to-first-/health.
// usage: node rust/bench/serve.mjs start --name a --port 3100 --cmd "node dist/server.js"
//        node rust/bench/serve.mjs stop --name a
import { spawn } from 'node:child_process';
import { closeSync, mkdirSync, openSync, readFileSync, rmSync, writeFileSync } from 'node:fs';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { serverEnv } from './bench-env.mjs';

const API_ROOT = resolve(dirname(fileURLToPath(import.meta.url)), '../..');
const RUN_DIR = '/tmp/kw-bench-run';
const [action, ...rest] = process.argv.slice(2);
const arg = (k, d) => (rest.includes(`--${k}`) ? rest[rest.indexOf(`--${k}`) + 1] : d);
const name = arg('name', 'a');
const stateFile = `${RUN_DIR}/${name}.json`;
mkdirSync(RUN_DIR, { recursive: true });

if (action === 'start') {
  const port = Number(arg('port', 3100));
  const cmd = arg('cmd').split(/\s+/);
  const log = openSync(`${RUN_DIR}/${name}.log`, 'w');
  const t0 = performance.now();
  const child = spawn(cmd[0].includes('/') ? resolve(API_ROOT, cmd[0]) : cmd[0], cmd.slice(1), {
    cwd: API_ROOT, // certs/ is resolved relative to cwd by the server
    env: { ...process.env, ...serverEnv(port) },
    detached: true,
    stdio: ['ignore', log, log],
  });
  child.unref();
  closeSync(log);
  let startMs = null;
  for (let i = 0; i < 3000 && startMs === null; i++) {
    try {
      const r = await fetch(`http://127.0.0.1:${port}/health`);
      if (r.ok) startMs = performance.now() - t0;
    } catch {}
    if (startMs === null) await new Promise((r) => setTimeout(r, 5));
  }
  if (startMs === null) {
    console.error(`server did not become healthy; see ${RUN_DIR}/${name}.log`);
    process.exit(1);
  }
  const state = { pid: child.pid, port, cmd: arg('cmd'), startMs: Math.round(startMs) };
  writeFileSync(stateFile, JSON.stringify(state));
  console.log(JSON.stringify(state));
} else if (action === 'stop') {
  try {
    const { pid } = JSON.parse(readFileSync(stateFile, 'utf8'));
    process.kill(pid, 'SIGTERM');
    rmSync(stateFile);
    console.log(`stopped ${name} (pid ${pid})`);
  } catch (e) {
    console.error(`nothing to stop for ${name}: ${e.message}`);
  }
} else {
  console.error('usage: serve.mjs start|stop --name a|b [--port N --cmd "..."]');
  process.exit(1);
}
