#!/usr/bin/env node
// Benchmark orchestrator. Measures server A (TS reference) and optionally B (Rust) on identical data, one at a time.
// usage: node rust/bench/run.mjs --a http://127.0.0.1:3100 --pid-a <pid> [--b http://127.0.0.1:3101 --pid-b <pid>]
//          [--only a|b] [--out rust/bench/results/<date>.json] [--duration 10] [--warmup 5] [--reps 3]
//          [--conc 1,16,64,256] [--scenarios health,jwks,guests,information,attendance,operational,operational-all(opt-in),incident-get,incident-post,login,ptt] [--ptt-clients 50,256] [--ptt-seconds 28]
//          [--label-a ts] [--label-b rust] [--artifact-a ts] [--artifact-b <path to binary>]
import { execFile, execFileSync } from 'node:child_process';
import { existsSync, mkdirSync, readFileSync, writeFileSync } from 'node:fs';
import os from 'node:os';
import { dirname, resolve } from 'node:path';
import { fileURLToPath } from 'node:url';
import { promisify } from 'node:util';
import { DATABASE_URL, REDIS_URL } from './bench-env.mjs';

const run = promisify(execFile);
const HERE = dirname(fileURLToPath(import.meta.url));
const API_ROOT = resolve(HERE, '../..');
const TOKENS = '/tmp/kw-bench-tokens.json';
const DB_SOCK = '/tmp/kw-bench.sock';
const REDIS_PORT = '6393';

const argv = process.argv.slice(2);
const arg = (k, d) => (argv.includes(`--${k}`) ? argv[argv.indexOf(`--${k}`) + 1] : d);
const opt = {
  duration: Number(arg('duration', 10)),
  warmup: Number(arg('warmup', 5)),
  reps: Number(arg('reps', 3)),
  conc: arg('conc', '1,16,64,256').split(',').map(Number),
  only: arg('only', null),
  pttClients: arg('ptt-clients', '50,256').split(',').map(Number),
  pttSeconds: Number(arg('ptt-seconds', 28)),
};
const date = new Date().toLocaleDateString('sv-SE'); // YYYY-MM-DD, local
const outJson = resolve(API_ROOT, arg('out', `rust/bench/results/${date}.json`));
const outMd = outJson.replace(/\.json$/, '.md');

const servers = ['a', 'b']
  .filter((id) => arg(id) && opt.only !== (id === 'a' ? 'b' : 'a'))
  .map((id) => ({
    id,
    url: arg(id).replace(/\/$/, ''),
    pid: Number(arg(`pid-${id}`)),
    label: arg(`label-${id}`, id === 'a' ? 'ts' : 'rust'),
    artifact: arg(`artifact-${id}`, id === 'a' ? 'ts' : null),
  }));
if (!servers.length) {
  console.error('usage: run.mjs --a <url> --pid-a <pid> [--b <url> --pid-b <pid>] [--only a|b] [--out file.json]');
  process.exit(1);
}
for (const s of servers) if (!s.pid) throw new Error(`--pid-${s.id} is required`);

const log = (...a) => console.error(`[bench ${new Date().toTimeString().slice(0, 8)}]`, ...a);
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const median = (xs) => {
  const v = xs.filter((x) => x !== null && x !== undefined && !Number.isNaN(x)).sort((a, b) => a - b);
  if (!v.length) return null;
  const m = v.length >> 1;
  return v.length % 2 ? v[m] : (v[m - 1] + v[m]) / 2;
};

// ---------- scratch services helpers ----------
const sql = (q) => execFileSync('mariadb', ['--no-defaults', '-uroot', `--socket=${DB_SOCK}`, 'kawalo_bench', '-e', q]);
const redisCli = (...a) => execFileSync('redis-cli', ['-p', REDIS_PORT, ...a]).toString();
const resetIncidents = () => sql('TRUNCATE tkincidentreply; TRUNCATE tkincident;');
const resetLoginState = () => {
  const keys = redisCli('--scan', '--pattern', 'auth:refresh:*').split('\n')
    .concat(redisCli('--scan', '--pattern', 'auth:family:*').split('\n'), redisCli('--scan', '--pattern', 'auth:user:*:sessions').split('\n'))
    .filter(Boolean);
  for (let i = 0; i < keys.length; i += 500) redisCli('DEL', ...keys.slice(i, i + 500));
};

// ---------- process sampler (ps rss + cpu time) ----------
const cpuSeconds = (t) => t.trim().split(':').reduce((acc, p) => acc * 60 + Number.parseFloat(p), 0);
function sampler(pid) {
  const samples = [];
  let stopped = false;
  const tick = async () => {
    try {
      const { stdout } = await run('ps', ['-o', 'rss=,time=', '-p', String(pid)]);
      const [rss, time] = stdout.trim().split(/\s+/);
      samples.push({ t: performance.now(), rssMb: Number(rss) / 1024, cpu: cpuSeconds(time) });
    } catch {}
  };
  const loop = (async () => {
    while (!stopped) { await tick(); await sleep(500); }
  })();
  return async () => {
    stopped = true;
    await loop;
    await tick();
    if (!samples.length) return { rssPeakMb: null, rssAvgMb: null, cpuPct: null };
    const first = samples[0];
    const last = samples.at(-1);
    const wall = (last.t - first.t) / 1000;
    return {
      rssPeakMb: Math.max(...samples.map((s) => s.rssMb)),
      rssAvgMb: samples.reduce((a, s) => a + s.rssMb, 0) / samples.length,
      cpuPct: wall > 0 ? ((last.cpu - first.cpu) / wall) * 100 : null, // 100 = one full core
    };
  };
}

// ---------- scenarios ----------
const tokens = () => JSON.parse(readFileSync(TOKENS, 'utf8'));
const auth = (t) => [`Authorization: Bearer ${t}`];
const scenarios = [
  { name: 'health', req: () => ({ method: 'GET', path: '/health' }) },
  { name: 'jwks', req: () => ({ method: 'GET', path: '/.well-known/jwks.json' }) },
  ...['guests', 'information', 'attendance'].map((n) => ({
    name: n,
    req: (c) => ({ method: 'GET', path: `/api/v1/${n}?limit=20`, headers: auth(c.tokens.guard) }),
  })),
  // The app always filters by date; the unbounded form is a full-group GROUP BY (~170 ms, DB-bound) kept opt-in.
  {
    name: 'operational',
    req: (c) => ({ method: 'GET', path: `/api/v1/operational?limit=20&start_date=${new Date(Date.now() - 7 * 864e5).toISOString()}`, headers: auth(c.tokens.guard) }),
  },
  { name: 'operational-all', optIn: true, req: (c) => ({ method: 'GET', path: '/api/v1/operational?limit=20', headers: auth(c.tokens.guard) }) },
  { name: 'incident-get', setup: true, req: (c) => ({ method: 'GET', path: `/api/v1/incidents/${c.incidentId}`, headers: auth(c.tokens.guard) }) },
  {
    name: 'incident-post',
    before: resetIncidents,
    req: (c) => ({
      method: 'POST',
      path: '/api/v1/incidents',
      headers: [...auth(c.tokens.guard), 'Content-Type: application/json'],
      body: JSON.stringify({
        title: 'Pagar belakang rusak', description: 'Pagar sisi timur roboh sebagian, perlu perbaikan segera.',
        category: 'facility', glat: -6.2, glong: 106.8, photos: ['https://cdn.bench.test/i/1.jpg'],
      }),
    }),
  },
  {
    name: 'login',
    before: resetLoginState,
    req: (c) => ({
      method: 'POST',
      path: '/api/v1/auth/login',
      headers: ['Content-Type: application/json'],
      body: JSON.stringify(c.tokens.login),
    }),
  },
  { name: 'ptt', ptt: true },
];
const wanted = arg('scenarios', null)?.split(',');
const active = scenarios.filter((s) => (wanted ? wanted.includes(s.name) : !s.optIn));

async function ohaRun(server, rq, conc, seconds) {
  const args = ['-z', `${seconds}s`, '-c', String(conc), '--no-tui', '-w', '--output-format', 'json', '-m', rq.method];
  for (const h of rq.headers ?? []) args.push('-H', h);
  if (rq.body) args.push('-d', rq.body);
  args.push(`${server.url}${rq.path}`);
  const { stdout } = await run('oha', args, { maxBuffer: 256 * 1024 * 1024 });
  const j = JSON.parse(stdout);
  const status = j.statusCodeDistribution ?? {};
  const ok = Object.entries(status).filter(([c]) => c.startsWith('2')).reduce((a, [, n]) => a + n, 0);
  const errs = Object.values(j.errorDistribution ?? {}).reduce((a, n) => a + n, 0);
  const total = Object.values(status).reduce((a, n) => a + n, 0) + errs;
  const ms = (s) => (s === null || s === undefined ? null : s * 1000);
  return {
    rps: j.summary.requestsPerSec,
    p50: ms(j.latencyPercentiles.p50), p90: ms(j.latencyPercentiles.p90), p99: ms(j.latencyPercentiles.p99),
    max: ms(j.summary.slowest), errPct: total ? ((total - ok) / total) * 100 : 100, requests: total,
    status,
  };
}

async function preflight(server, ctx) {
  const get = (p, t) => fetch(`${server.url}${p}`, { headers: t ? { Authorization: `Bearer ${t}` } : {} });
  if (!(await get('/health')).ok) throw new Error(`${server.label}: /health failed`);
  const g = await get('/api/v1/guests?limit=20', ctx.tokens.guard);
  if (!g.ok) throw new Error(`${server.label}: authenticated GET /guests -> ${g.status} (tokens/seed/certs mismatch?)`);
  const n = (await g.json()).data?.length;
  if (n !== 20) throw new Error(`${server.label}: /guests returned ${n} rows, expected 20 (seed missing?)`);
}

async function createIncidentFixture(server, ctx) {
  resetIncidents();
  const h = { Authorization: `Bearer ${ctx.tokens.guard}`, 'Content-Type': 'application/json' };
  const r = await fetch(`${server.url}/api/v1/incidents`, {
    method: 'POST', headers: h, body: JSON.stringify({ title: 'Fixture insiden', description: 'Untuk benchmark detail', category: 'facility' }),
  });
  if (r.status !== 201) throw new Error(`${server.label}: create incident -> ${r.status}`);
  const id = (await r.json()).data.id;
  for (let i = 0; i < 10; i++) {
    await fetch(`${server.url}/api/v1/incidents/${id}/replies`, { method: 'POST', headers: h, body: JSON.stringify({ message: `Balasan ke-${i + 1}`, photos: [] }) });
  }
  return id;
}

// ---------- artifact size ----------
const duKb = (...paths) => Number(execFileSync('du', ['-sk', ...paths]).toString().split('\n').filter(Boolean).reduce((a, l) => a + Number(l.split('\t')[0]), 0));
function artifactSize(spec) {
  if (!spec) return null;
  if (spec === 'ts') {
    const dist = resolve(API_ROOT, 'dist');
    // npm ls exits non-zero on unrelated tree warnings but still prints the paths.
    let out;
    try { out = execFileSync('npm', ['ls', '--omit=dev', '--all', '--parseable'], { cwd: API_ROOT, maxBuffer: 1 << 26, stdio: ['ignore', 'pipe', 'ignore'] }); } catch (e) { out = e.stdout; }
    const paths = out.toString().split('\n').filter((p) => p.includes('/node_modules/'));
    const top = paths.filter((p) => !paths.some((q) => q !== p && p.startsWith(`${q}/`)));
    return { kind: 'dist + production node_modules', distMb: duKb(dist) / 1024, nodeModulesMb: duKb(...top) / 1024, totalMb: (duKb(dist) + duKb(...top)) / 1024 };
  }
  const p = resolve(API_ROOT, spec);
  return existsSync(p) ? { kind: 'binary', totalMb: duKb(p) / 1024 } : { kind: 'missing', path: spec };
}

// ---------- machine ----------
const sh = (c, a) => { try { return execFileSync(c, a, { stdio: ['ignore', 'pipe', 'ignore'] }).toString().trim(); } catch { return 'n/a'; } };
const machine = {
  cpu: sh('sysctl', ['-n', 'machdep.cpu.brand_string']), cores: os.cpus().length,
  ramGb: Math.round(os.totalmem() / 2 ** 30), os: `${os.type()} ${os.release()} ${os.arch()}`,
  node: process.version, mariadb: sh('mariadbd', ['--version']), redis: sh('redis-server', ['--version']), oha: sh('oha', ['--version']),
};

// ---------- main ----------
const startedAt = new Date().toISOString();
log('refreshing bench tokens');
execFileSync(resolve(API_ROOT, 'node_modules/.bin/tsx'), [resolve(HERE, 'mktokens.mts'), TOKENS], {
  cwd: API_ROOT, stdio: ['ignore', 'ignore', 'inherit'],
  env: { ...process.env, REDIS_URL, NODE_ENV: 'production', JWT_EXPIRES_IN: '604800', LOG_LEVEL: 'warn', DATABASE_URL },
});
const ctx = { tokens: tokens(), incidentId: null };

const result = { date, startedAt, loadAvgStart: os.loadavg(), machine, options: opt, servers: {}, scenarios: [] };
for (const s of servers) {
  let startMs = null;
  const st = `/tmp/kw-bench-run/${s.id}.json`;
  if (existsSync(st)) { const j = JSON.parse(readFileSync(st, 'utf8')); if (j.pid === s.pid) startMs = j.startMs; }
  log(`${s.label}: idle RSS sampling (3s)`);
  const stop = sampler(s.pid);
  await sleep(3000);
  const idle = await stop();
  result.servers[s.id] = { label: s.label, url: s.url, pid: s.pid, startMs, idleRssMb: idle.rssAvgMb, artifact: artifactSize(s.artifact) };
  await preflight(s, ctx);
}

for (const sc of active) {
  if (sc.ptt) {
    for (const clients of opt.pttClients) {
      const row = { scenario: 'ptt', conc: clients, servers: {} };
      for (let rep = 0; rep < opt.reps; rep++) {
        for (const s of servers) {
          log(`ptt ${clients} clients rep ${rep + 1}/${opt.reps} on ${s.label}`);
          const stop = sampler(s.pid);
          const { stdout } = await run('node', [resolve(HERE, 'ptt-bench.mjs'), '--url', s.url.replace(/^http/, 'ws'), '--clients', String(clients), '--seconds', String(opt.pttSeconds), '--tokens', TOKENS], { maxBuffer: 1 << 26 });
          const res = { ...JSON.parse(stdout), ...(await stop()) };
          (row.servers[s.id] ??= { reps: [] }).reps.push(res);
          await sleep(3000); // server drops presence/sockets
        }
      }
      for (const s of servers) {
        const reps = row.servers[s.id].reps;
        const m = (f) => median(reps.map(f));
        row.servers[s.id].median = {
          connectMs: m((r) => r.connectMs), floorGrantMs: m((r) => r.floorGrantMs), lossPct: m((r) => r.lossPct), lost: m((r) => r.lost),
          deliveredPerSec: m((r) => r.deliveredPerSec), p50: m((r) => r.latencyMs.p50), p90: m((r) => r.latencyMs.p90),
          p99: m((r) => r.latencyMs.p99), max: m((r) => r.latencyMs.max), rssPeakMb: m((r) => r.rssPeakMb), rssAvgMb: m((r) => r.rssAvgMb), cpuPct: m((r) => r.cpuPct),
        };
      }
      result.scenarios.push(row);
    }
    continue;
  }

  if (sc.setup) ctx.incidentId = await createIncidentFixture(servers[0], ctx);
  for (const conc of opt.conc) {
    const row = { scenario: sc.name, conc, servers: {} };
    for (let rep = 0; rep < opt.reps; rep++) {
      for (const s of servers) {
        const rq = sc.req(ctx);
        if (rep === 0 && opt.warmup > 0) {
          log(`${sc.name} c=${conc} warm-up on ${s.label}`);
          sc.before?.();
          await ohaRun(s, rq, conc, opt.warmup);
        }
        log(`${sc.name} c=${conc} rep ${rep + 1}/${opt.reps} on ${s.label}`);
        sc.before?.();
        const stop = sampler(s.pid);
        const r = await ohaRun(s, rq, conc, opt.duration);
        (row.servers[s.id] ??= { reps: [] }).reps.push({ ...r, ...(await stop()) });
      }
    }
    for (const s of servers) {
      const reps = row.servers[s.id].reps;
      const m = (f) => median(reps.map(f));
      const med = {
        rps: m((r) => r.rps), p50: m((r) => r.p50), p90: m((r) => r.p90), p99: m((r) => r.p99), max: m((r) => r.max),
        errPct: m((r) => r.errPct), rssPeakMb: m((r) => r.rssPeakMb), rssAvgMb: m((r) => r.rssAvgMb), cpuPct: m((r) => r.cpuPct),
      };
      med.rpsPerCpuCore = med.cpuPct ? med.rps / (med.cpuPct / 100) : null;
      row.servers[s.id].median = med;
    }
    result.scenarios.push(row);
    // Persist incrementally so a long run survives an interruption.
    mkdirSync(dirname(outJson), { recursive: true });
    writeFileSync(outJson, JSON.stringify(result, null, 1));
  }
}

for (const s of servers) {
  const stop = sampler(s.pid);
  await sleep(1000);
  result.servers[s.id].endRssMb = (await stop()).rssAvgMb;
}
result.finishedAt = new Date().toISOString();
result.loadAvgEnd = os.loadavg();
mkdirSync(dirname(outJson), { recursive: true });
writeFileSync(outJson, JSON.stringify(result, null, 1));

// ---------- markdown ----------
const f = (x, d = 1) => (x === null || x === undefined ? '-' : x.toFixed(d));
const ids = servers.map((s) => s.id);
const L = (id) => result.servers[id].label;
const md = [];
md.push(`# Benchmark ${date}`, '');
md.push(`Servers: ${ids.map((id) => `**${L(id)}** (${result.servers[id].url}, pid ${result.servers[id].pid})`).join(' vs ')}`, '');
md.push('## Machine', '', '| | |', '|---|---|');
for (const [k, v] of Object.entries(machine)) md.push(`| ${k} | ${v} |`);
md.push('', `Method: oha closed-loop, keep-alive, ${opt.duration}s per run after ${opt.warmup}s warm-up (discarded), median of ${opt.reps} runs; servers measured one at a time (interleaved per repetition) against the same MariaDB/Redis scratch instances. Load generator runs on the same machine.`, '');
md.push('## Process footprint', '', `| | ${ids.map(L).join(' | ')} |`, `|---|${ids.map(() => '---').join('|')}|`);
md.push(`| start to first /health (ms) | ${ids.map((id) => result.servers[id].startMs ?? '-').join(' | ')} |`);
md.push(`| idle RSS (MB) | ${ids.map((id) => f(result.servers[id].idleRssMb)).join(' | ')} |`);
md.push(`| RSS after all runs (MB) | ${ids.map((id) => f(result.servers[id].endRssMb)).join(' | ')} |`);
md.push(`| artifact size (MB) | ${ids.map((id) => { const a = result.servers[id].artifact; return a ? `${f(a.totalMb)}${a.nodeModulesMb ? ` (dist ${f(a.distMb)} + node_modules ${f(a.nodeModulesMb)})` : ''}` : '-'; }).join(' | ')} |`);
md.push('');
md.push('## HTTP scenarios', '');
const head = ['scenario', 'c'];
for (const id of ids) head.push(...['rps', 'p50 ms', 'p90 ms', 'p99 ms', 'max ms', 'err %', 'RSS peak MB', 'CPU %'].map((h) => `${L(id)} ${h}`));
if (ids.length === 2) head.push(`rps ${L(ids[1])}/${L(ids[0])}`, `p99 ${L(ids[0])}/${L(ids[1])}`);
md.push(`| ${head.join(' | ')} |`, `|${head.map(() => '---').join('|')}|`);
for (const row of result.scenarios.filter((r) => r.scenario !== 'ptt')) {
  const cells = [row.scenario, row.conc];
  for (const id of ids) {
    const m = row.servers[id].median;
    cells.push(f(m.rps, 0), f(m.p50, 2), f(m.p90, 2), f(m.p99, 2), f(m.max, 1), f(m.errPct, 2), f(m.rssPeakMb, 0), f(m.cpuPct, 0));
  }
  if (ids.length === 2) {
    const [a, b] = ids.map((id) => row.servers[id].median);
    cells.push(`${f(b.rps / a.rps, 2)}x`, `${f(a.p99 / b.p99, 2)}x`);
  }
  md.push(`| ${cells.join(' | ')} |`);
}
const ptt = result.scenarios.filter((r) => r.scenario === 'ptt');
if (ptt.length) {
  md.push('', `## PTT WebSocket fan-out (1 speaker, N-1 listeners, 50 fps x ${opt.pttSeconds}s, 67-byte frames)`, '');
  const ph = ['clients'];
  for (const id of ids) ph.push(...['p50 ms', 'p90 ms', 'p99 ms', 'max ms', 'loss %', 'delivered/s', 'connect ms', 'RSS peak MB', 'CPU %'].map((h) => `${L(id)} ${h}`));
  md.push(`| ${ph.join(' | ')} |`, `|${ph.map(() => '---').join('|')}|`);
  for (const row of ptt) {
    const cells = [row.conc];
    for (const id of ids) {
      const m = row.servers[id].median;
      cells.push(f(m.p50, 2), f(m.p90, 2), f(m.p99, 2), f(m.max, 1), f(m.lossPct, 2), f(m.deliveredPerSec, 0), f(m.connectMs, 0), f(m.rssPeakMb, 0), f(m.cpuPct, 0));
    }
    md.push(`| ${cells.join(' | ')} |`);
  }
}
md.push('', `Host load average (1/5/15 min) at start: ${result.loadAvgStart.map((x) => x.toFixed(2)).join(' / ')}; at end: ${result.loadAvgEnd.map((x) => x.toFixed(2)).join(' / ')}. Other CPU-heavy work on the host (builds, other agents) skews results.`, '');
md.push('CPU % is process CPU time over wall time (100 = one full core). RSS sampled with `ps` every 500 ms. Raw repetitions are in the JSON.', '');
writeFileSync(outMd, md.join('\n'));
console.log(md.join('\n'));
log(`wrote ${outJson} and ${outMd}`);
