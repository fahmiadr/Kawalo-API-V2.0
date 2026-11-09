#!/usr/bin/env node
// Parity runner: replays the same scenarios against server A (TypeScript, reference) and server B (Rust)
// and reports every difference in status, contract headers and normalized body.
//   node tools/parity/run.mjs --a http://127.0.0.1:3410 --b http://127.0.0.1:3411 [--only guest,incident]
//        [--skip ptt] [--verbose] [--slow] [--ignore-details] [--fail-fast] [--json out.json] [--tokens file] [--redis urlA,urlB] [--list]
// Prefer `tools/parity/services.sh run`, which resets both databases and Redis first.
import { spawnSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, readdirSync, symlinkSync, writeFileSync } from 'node:fs';
import { dirname, join, resolve } from 'node:path';
import { fileURLToPath, pathToFileURL } from 'node:url';
import { Harness } from './lib/harness.mjs';

const here = dirname(fileURLToPath(import.meta.url));
const repo = resolve(here, '../..');
// Checkout of the legacy/typescript branch: the oracle server and the token minter run from it (see tools/README.md).
const tsRoot = resolve(process.env.KW_TS_ROOT ?? join(repo, '..', 'Kawalo-API-V2.0-ts'));

function parseArgs(argv) {
  const o = { flags: new Set() };
  for (let i = 0; i < argv.length; i++) {
    const a = argv[i];
    if (!a.startsWith('--')) continue;
    const k = a.slice(2);
    const next = argv[i + 1];
    if (next !== undefined && !next.startsWith('--')) {
      o[k] = next;
      i++;
    } else o.flags.add(k);
  }
  return o;
}

const args = parseArgs(process.argv.slice(2));
const csv = (v) => (v ? v.split(',').map((s) => s.trim()).filter(Boolean) : []);

async function loadScenarios() {
  const files = readdirSync(join(here, 'scenarios')).filter((f) => f.endsWith('.mjs')).sort();
  const defs = [];
  for (const f of files) {
    const mod = await import(pathToFileURL(join(here, 'scenarios', f)).href);
    for (const d of [].concat(mod.default)) defs.push({ file: f, ...d });
  }
  return defs.sort((x, y) => (x.order ?? 50) - (y.order ?? 50));
}

function mintTokens() {
  const base = process.env.KW_BASE ?? '/tmp/kw-parity';
  const cwd = join(base, 'cwd');
  mkdirSync(cwd, { recursive: true });
  if (!existsSync(join(cwd, 'certs'))) symlinkSync(join(repo, 'certs'), join(cwd, 'certs'));
  const out = join(here, '.cache', 'tokens.json');
  mkdirSync(dirname(out), { recursive: true });
  const tsx = join(tsRoot, 'node_modules/.bin/tsx');
  if (!existsSync(tsx)) throw new Error(`TypeScript oracle not found at ${tsRoot} (set KW_TS_ROOT; see tools/README.md)`);
  // mint-token.mts lives outside the oracle checkout; this link lets it resolve ioredis/jose from there.
  const deps = join(here, '../node_modules');
  if (!existsSync(deps)) symlinkSync(join(tsRoot, 'node_modules'), deps);
  const r = spawnSync(
    tsx,
    ['--tsconfig', join(tsRoot, 'tsconfig.json'), join(here, 'mint-token.mts'), '--redis', args.redis ?? '', '--out', out],
    { cwd, encoding: 'utf8' },
  );
  if (r.status !== 0) {
    console.error(r.stdout, r.stderr);
    throw new Error('token minting failed');
  }
  return JSON.parse(readFileSync(out, 'utf8'));
}

async function healthy(base) {
  try {
    return (await fetch(`${base}/health`)).ok;
  } catch {
    return false;
  }
}

async function main() {
  let defs = await loadScenarios();
  if (args.flags.has('list')) {
    for (const d of defs) console.log(`${d.name.padEnd(28)} ${d.file}${d.slow ? '  (slow)' : ''}`);
    return 0;
  }
  const only = csv(args.only);
  const skip = csv(args.skip);
  if (only.length) defs = defs.filter((d) => only.some((o) => d.name.startsWith(o) || d.file.replace('.mjs', '') === o));
  defs = defs.filter((d) => !skip.some((o) => d.name.startsWith(o) || d.file.replace('.mjs', '') === o));
  const slow = args.flags.has('slow');
  defs = defs.filter((d) => slow || !d.slow);

  const a = args.a ?? process.env.KW_URL_A;
  const b = args.b ?? process.env.KW_URL_B;
  if (!a || !b) {
    console.error('usage: node run.mjs --a <url of TS server> --b <url of Rust server> [--only x,y] [--verbose] [--slow]');
    return 2;
  }
  for (const [n, u] of [['A', a], ['B', b]]) {
    if (!(await healthy(u))) {
      console.error(`server ${n} (${u}) is not answering /health`);
      return 2;
    }
  }
  const tokens = args.tokens ? JSON.parse(readFileSync(args.tokens, 'utf8')) : mintTokens();

  const h = new Harness({ a, b, tokens, verbose: args.flags.has('verbose'), failFast: args.flags.has('fail-fast'), slow, ignoreDetails: args.flags.has('ignore-details') });
  console.log(`parity: A=${a} (reference)  B=${b}  scenarios=${defs.length}\n`);
  const t0 = Date.now();
  let idx = 0;
  for (const d of defs) {
    const r = await h.runScenario(d, idx++);
    const bad = r.steps.filter((s) => !s.ok).length;
    const status = r.crashed ? 'CRASH' : bad || r.warnings.length ? 'FAIL ' : 'PASS ';
    console.log(`${status} ${d.name.padEnd(30)} ${String(r.steps.length).padStart(3)} steps${bad ? `, ${bad} differ` : ''}${r.warnings.length ? `, ${r.warnings.length} setup warnings` : ''}`);
    if (r.crashed) console.log(`      scenario error: ${r.crashed.split('\n').slice(0, 4).join('\n      ')}`);
    if (bad && h.cfg.failFast) break;
  }

  const total = h.results.reduce((n, r) => n + r.steps.length, 0);
  const bad = h.results.reduce((n, r) => n + r.steps.filter((s) => !s.ok).length, 0);
  const warns = h.results.reduce((n, r) => n + r.warnings.length, 0);
  const crashed = h.results.filter((r) => r.crashed).length;
  console.log(`\n${total} steps, ${bad} differ, ${warns} setup warnings, ${crashed} crashed scenarios in ${((Date.now() - t0) / 1000).toFixed(1)}s`);
  if (args.json) {
    writeFileSync(args.json, JSON.stringify(h.results.map((r) => ({ ...r, steps: r.steps.map(({ label, ok, diffs, desc, a, b }) => ({ label, ok, desc, diffs, a, b })) })), null, 2));
  }
  return bad || warns || crashed ? 1 : 0;
}

process.exit(await main());
