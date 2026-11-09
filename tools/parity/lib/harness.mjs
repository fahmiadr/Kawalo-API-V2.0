import { Normalizer, diff, show } from './normalize.mjs';
import { wsHandshake } from './ws.mjs';

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

// Response headers that are part of the contract. Compared exactly unless listed in APPROX.
const EXACT_HEADERS = [
  'content-type',
  'x-content-type-options',
  'x-frame-options',
  'cache-control',
  'access-control-allow-origin',
  'access-control-allow-credentials',
  'access-control-expose-headers',
  'access-control-allow-headers',
  'access-control-allow-methods',
  'access-control-max-age',
  'vary',
  'ratelimit-limit',
  'ratelimit-remaining',
  'x-accel-buffering',
  'allow',
  'www-authenticate',
  'content-disposition',
  'location',
  'sec-websocket-protocol',
  'sec-websocket-accept',
  'upgrade',
];
// Seconds counters that legitimately drift by a tick between the two servers.
const APPROX_HEADERS = ['retry-after', 'ratelimit-reset'];
// Transport-level headers; the set of remaining header names is still compared.
const IGNORED_NAMES = new Set(['date', 'content-length', 'connection', 'keep-alive', 'transfer-encoding']);

const resolve = (x, v) => (typeof x === 'function' ? x(v) : x);
const dig = (obj, path) => path.split('.').reduce((o, k) => (o == null ? undefined : o[k]), obj);

function headerSnapshot(headers, supplied) {
  const out = {};
  const names = [];
  for (const [k, v] of headers) {
    const name = k.toLowerCase();
    if (!IGNORED_NAMES.has(name)) names.push(name);
    if (EXACT_HEADERS.includes(name)) out[name] = name === 'content-type' ? v.toLowerCase().replace(/;\s+/g, ';') : v;
    if (APPROX_HEADERS.includes(name)) out[name] = Number(v);
    if (name === 'x-request-id') out[name] = supplied && v === supplied ? '<echo>' : '<present>';
  }
  out.$names = names.sort();
  return out;
}

function diffApprox(a, b, out) {
  for (const h of APPROX_HEADERS) {
    const x = a.headers[h];
    const y = b.headers[h];
    if (x === undefined && y === undefined) continue;
    if (typeof x === 'number' && typeof y === 'number' && Math.abs(x - y) <= 3) {
      delete a.headers[h];
      delete b.headers[h];
    }
  }
  return out;
}

export class Server {
  constructor({ name, base, tokens, ip, jwks, runStartMs, ignoreDetails = false }) {
    this.name = name;
    this.ignoreDetails = ignoreDetails;
    this.base = base.replace(/\/$/, '');
    this.tokens = tokens;
    this.ip = ip;
    this.vars = {};
    this.norm = new Normalizer({ jwks, runStartMs });
  }

  tokenValue(name) {
    if (!name) return null;
    if (name.startsWith('raw:')) return name.slice(4);
    const t = this.tokens[name];
    if (t === undefined) throw new Error(`unknown token "${name}"`);
    return t;
  }

  buildRequest(spec) {
    const v = this.vars;
    let path = resolve(spec.path, v);
    const query = resolve(spec.query, v);
    if (query) {
      const qs = new URLSearchParams();
      for (const [k, val] of Object.entries(query)) {
        if (val === undefined) continue;
        for (const item of Array.isArray(val) ? val : [val]) qs.append(k, String(item));
      }
      const s = qs.toString();
      if (s) path += (path.includes('?') ? '&' : '?') + s;
    }
    const headers = { 'x-forwarded-for': spec.ip ?? this.ip, ...(resolve(spec.headers, v) ?? {}) };
    const token = this.tokenValue(resolve(spec.token, v));
    if (token) headers.authorization = `Bearer ${token}`;
    let body;
    if (spec.rawBody !== undefined) {
      body = resolve(spec.rawBody, v);
      headers['content-type'] ??= 'application/json';
    } else if (spec.body !== undefined) {
      body = JSON.stringify(resolve(spec.body, v));
      headers['content-type'] ??= 'application/json';
    }
    return { url: this.base + path, init: { method: spec.method ?? 'GET', headers, body, redirect: 'manual' }, headers };
  }

  /** One HTTP exchange against this server; returns the raw response and its normalized snapshot. */
  async call(spec) {
    const { url, init, headers } = this.buildRequest(spec);
    const res = await fetch(url, init);
    const text = await res.text();
    const ct = res.headers.get('content-type') ?? '';
    let json;
    let body;
    if (text === '') body = null;
    else if (ct.includes('json')) {
      try {
        json = JSON.parse(text);
        body = this.norm.value(json);
      } catch {
        body = { $unparseable: this.norm.string(text) };
      }
    } else body = { $text: this.norm.string(text) };
    for (const p of spec.ignore ?? []) body = omit(body, p.split('.'));
    if (this.ignoreDetails && body && body.success === false) delete body.details;
    const snapshot = { status: res.status, headers: headerSnapshot(res.headers, headers['x-request-id']), body };
    for (const [name, path] of Object.entries(spec.capture ?? {})) {
      this.vars[name] = typeof path === 'function' ? path({ json, text, res }) : dig(json, path);
    }
    for (const [name, header] of Object.entries(spec.captureHeader ?? {})) this.vars[name] = res.headers.get(header);
    return { snapshot, json, text, status: res.status, res };
  }

  /** Server-sent events: opens the stream, then lets `script` drive requests and read frames. */
  async sse(spec, script) {
    const { url, init } = this.buildRequest({ ...spec, headers: { accept: 'text/event-stream', ...(spec.headers ?? {}) } });
    const ac = new AbortController();
    const res = await fetch(url, { ...init, signal: ac.signal });
    const log = [];
    const head = { status: res.status, headers: headerSnapshot(res.headers) };
    log.push({ type: 'open', ...head });
    if (!(res.headers.get('content-type') ?? '').includes('event-stream')) {
      const text = await res.text();
      let body = text;
      try { body = this.norm.value(JSON.parse(text)); } catch { /* keep text */ }
      if (this.ignoreDetails && body && body.success === false) delete body.details;
      log.push({ type: 'body', body });
      return log;
    }
    const frames = [];
    const waiters = [];
    const pushFrame = (f) => (waiters.length ? waiters.shift()(f) : frames.push(f));
    (async () => {
      const dec = new TextDecoder();
      let buf = '';
      try {
        for await (const chunk of res.body) {
          buf += dec.decode(chunk, { stream: true });
          let i;
          while ((i = buf.indexOf('\n\n')) >= 0) {
            const raw = buf.slice(0, i);
            buf = buf.slice(i + 2);
            pushFrame(this.#parseFrame(raw));
          }
        }
      } catch { /* aborted */ }
      pushFrame({ type: 'end' });
    })();
    const next = (ms) =>
      new Promise((resolveNext) => {
        if (frames.length) return resolveNext(frames.shift());
        const t = setTimeout(() => {
          waiters.splice(waiters.indexOf(w), 1);
          resolveNext({ type: 'timeout' });
        }, ms);
        const w = (f) => (clearTimeout(t), resolveNext(f));
        waiters.push(w);
      });
    const s = {
      srv: this,
      sleep,
      /** Reads the next frame (strict order) and records it. */
      async expect(label, ms = 3000) {
        const f = await next(ms);
        log.push({ type: 'frame', label, ...f });
        return f;
      },
      /** Reads frames until `pred` matches; skipped frames are recorded as well. */
      async expectUntil(label, pred, ms = 5000) {
        const t0 = Date.now();
        for (;;) {
          const f = await next(Math.max(50, ms - (Date.now() - t0)));
          log.push({ type: 'frame', label, ...f });
          if (f.type === 'timeout' || f.type === 'end' || pred(f)) return f;
        }
      },
      async silence(label, ms = 400) {
        const f = await next(ms);
        log.push({ type: 'silence', label, got: f.type === 'timeout' ? null : f });
      },
      /** Heartbeat timing is non-deterministic to the millisecond; only the 14-17s window is recorded. */
      async heartbeat(label) {
        const t0 = Date.now();
        const f = await next(20000);
        const dt = Date.now() - t0;
        log.push({ type: 'heartbeat', label, frame: f, inWindow: dt >= 13000 && dt <= 17500 });
      },
      async req(label, reqSpec) {
        const r = await this.srv.call(reqSpec);
        log.push({ type: 'request', label, status: r.snapshot.status, body: r.snapshot.body });
        return r;
      },
      rec(label, value) {
        log.push({ type: 'note', label, value: this.srv.norm.value(value) });
      },
      close() {
        ac.abort();
      },
    };
    try {
      await script(s);
    } finally {
      ac.abort();
    }
    return log;
  }

  #parseFrame(raw) {
    const f = { type: 'event' };
    const data = [];
    for (const line of raw.split('\n')) {
      if (line.startsWith(':')) f.comment = line.slice(1).trim();
      else if (line.startsWith('id:')) f.id = this.norm.string(line.slice(3).trim());
      else if (line.startsWith('event:')) f.event = line.slice(6).trim();
      else if (line.startsWith('retry:')) f.retry = line.slice(6).trim();
      else if (line.startsWith('data:')) data.push(line.slice(5).replace(/^ /, ''));
    }
    if (data.length) {
      const joined = data.join('\n');
      try { f.data = this.norm.value(JSON.parse(joined)); } catch { f.data = joined; }
    }
    if (f.comment !== undefined && !f.event && !data.length) f.type = 'comment';
    return f;
  }

  /** WebSocket session; every received frame is appended to `log` in the order the script reads them. */
  async ws(name, path, { token, via = 'query', headers = {}, expectReject = false } = {}, log) {
    const t = this.tokenValue(token);
    const h = { 'x-forwarded-for': this.ip, ...headers };
    let url = this.base + path;
    if (t) {
      if (via === 'query') url += `${path.includes('?') ? '&' : '?'}token=${encodeURIComponent(t)}`;
      else if (via === 'header') h.authorization = `Bearer ${t}`;
      else if (via === 'header-raw') h.authorization = t;
      else if (via === 'protocol') h['sec-websocket-protocol'] = t;
    }
    const hs = await wsHandshake(url, { headers: h });
    const snap = { status: hs.status, headers: headerSnapshot(new Map(Object.entries(hs.headers))) };
    delete snap.headers['sec-websocket-protocol'];
    if (hs.headers['sec-websocket-protocol'] !== undefined) snap.headers.protocolEcho = hs.headers['sec-websocket-protocol'] === t ? '<token>' : hs.headers['sec-websocket-protocol'];
    // The TS upgrade response carries framework leftovers (content-type: text/plain, cors, ratelimit); only the handshake itself is contract.
    if (hs.status === 101) snap.headers = { upgrade: snap.headers.upgrade, ...(snap.headers.protocolEcho ? { protocolEcho: snap.headers.protocolEcho } : {}) };
    snap.acceptOk = hs.acceptOk ?? null;
    if (hs.body !== undefined) snap.body = this.norm.string(hs.body);
    log.push({ ws: name, type: 'handshake', ...snap });
    const norm = this.norm;
    const c = hs.client;
    const render = (m) => {
      if (m.kind === 'text') {
        try { return { text: norm.value(JSON.parse(m.text)) }; } catch { return { text: m.text }; }
      }
      if (m.kind === 'binary') return { binary: m.hex, length: m.length };
      if (m.kind === 'close') return { close: m.code, reason: m.reason };
      return { [m.kind]: true };
    };
    return {
      open: Boolean(c),
      send: (o) => c.sendJson(o),
      sendText: (s) => c.sendText(s),
      sendBinary: (b) => c.sendBinary(b),
      async expect(label, ms = 3000) {
        const m = await c.next(ms);
        log.push({ ws: name, type: 'recv', label, ...render(m) });
        return m;
      },
      async silence(label, ms = 400) {
        const m = await c.next(ms);
        log.push({ ws: name, type: 'silence', label, got: m.kind === 'timeout' ? null : render(m) });
      },
      close: (code) => c.close(code),
      async closed(label, ms = 3000) {
        const info = await Promise.race([c.closed, sleep(ms).then(() => ({ timeout: true }))]);
        log.push({ ws: name, type: 'closed', label, ...info });
      },
      /** Polite close so the server runs its disconnect handler before the next scenario starts. */
      async destroy() {
        if (!c) return;
        try { c.close(1000); } catch { /* socket already gone */ }
        await Promise.race([c.closed, sleep(600)]);
        c.destroy();
      },
    };
  }
}

export class Harness {
  constructor({ a, b, tokens, verbose = false, failFast = false, slow = false, ignoreDetails = false }) {
    this.cfg = { a, b, tokens, verbose, failFast, slow, ignoreDetails };
    this.results = [];
    this.runStartMs = Date.now();
    this.nonce = Math.floor(Math.random() * 200) + 20;
  }

  async #jwks(base) {
    try {
      const r = await fetch(`${base}/.well-known/jwks.json`);
      return await r.json();
    } catch {
      return null;
    }
  }

  async runScenario(def, index) {
    const { a, b, tokens, verbose } = this.cfg;
    const ip = `198.51.${this.nonce}.${index + 1}`;
    const [jwksA, jwksB] = await Promise.all([this.#jwks(a), this.#jwks(b)]);
    const { ignoreDetails } = this.cfg;
    const A = new Server({ name: 'A', base: a, tokens, ip, jwks: jwksA, runStartMs: this.runStartMs, ignoreDetails });
    const B = new Server({ name: 'B', base: b, tokens, ip, jwks: jwksB, runStartMs: this.runStartMs, ignoreDetails });
    const res = { name: def.name, steps: [], warnings: [], crashed: null };
    const t = {
      A,
      B,
      slow: this.cfg.slow,
      sleep,
      ip,
      tokens,
      /** Extra client ips for one scenario so ip-based limits (login throttle) never leak between blocks. */
      ipFor: (k) => `198.18.${index + 1}.${k}`,
      /** Runs `fn(server)` on both servers in parallel and compares the (already normalized) results. */
      compute: async (label, fn, opts = {}) => {
        const settle = (p) => p.then((value) => ({ value }), (error) => ({ error: `${error?.name}: ${error?.message}` }));
        const [ra, rb] = await Promise.all([settle(fn(A)), settle(fn(B))]);
        const va = ra.error ? { $error: ra.error } : ra.value;
        const vb = rb.error ? { $error: rb.error } : rb.value;
        const aa = structuredClone(va);
        const bb = structuredClone(vb);
        if (aa?.headers && bb?.headers) diffApprox(aa, bb, []);
        const diffs = diff(aa, bb);
        if (ra.error) {
          res.warnings.push(`${label}: reference (A) failed: ${ra.error}`);
          console.log(`  WARN [${def.name}] ${label}: reference (A) failed: ${ra.error}`);
        }
        const step = { label, ok: diffs.length === 0, diffs, desc: opts.desc ?? '', a: va, b: vb };
        res.steps.push(step);
        if (verbose || !step.ok) printStep(def.name, step, verbose);
        await sleep(3);
        return { a: ra.value, b: rb.value, step };
      },
      /** A single HTTP exchange compared across servers. `spec.expect` is the status the oracle (A) should return. */
      step: async (label, spec) => {
        const desc = `${spec.method ?? 'GET'} ${typeof spec.path === 'function' ? '(fn)' : spec.path}`;
        const raws = {};
        const r = await t.compute(label, async (srv) => {
          raws[srv.name] = await srv.call(spec);
          return raws[srv.name].snapshot;
        }, { desc });
        const a = raws.A;
        if (spec.expect !== undefined && a && a.status !== spec.expect) {
          res.warnings.push(`${label}: expected status ${spec.expect} on A, got ${a.status}`);
          console.log(`  WARN [${def.name}] ${label}: expected ${spec.expect} on reference, got ${a.status}`);
        }
        return { a, b: raws.B, step: r.step };
      },
      /** Strict-order SSE session. */
      sse: (label, spec, script) => t.compute(label, (srv) => srv.sse(spec, script), { desc: `SSE ${spec.path}` }),
      /** Free-form WebSocket session; `script(srv, log)` calls srv.ws(name, path, opts, log). */
      wsSession: (label, script) => t.compute(label, async (srv) => {
        const log = [];
        try { await script(srv, log); } finally { /* sockets are destroyed by the script */ }
        return log;
      }, { desc: 'WS' }),
      vars: { a: A.vars, b: B.vars },
    };
    try {
      await def.run(t);
    } catch (e) {
      res.crashed = e?.stack ?? String(e);
    }
    this.results.push(res);
    return res;
  }
}

function omit(obj, path) {
  if (obj == null || typeof obj !== 'object') return obj;
  const [head, ...rest] = path;
  if (Array.isArray(obj)) return obj.map((x) => omit(x, path));
  const copy = { ...obj };
  if (rest.length === 0) delete copy[head];
  else if (head in copy) copy[head] = omit(copy[head], rest);
  return copy;
}

function printStep(scenario, step, verbose) {
  const mark = step.ok ? 'ok  ' : 'FAIL';
  const where = step.desc ? ` (${step.desc})` : '';
  console.log(`  ${mark} ${scenario} > ${step.label}${where}`);
  if (!step.ok) {
    const shown = step.diffs.slice(0, 14);
    for (const d of shown) console.log(`       ${d.path}: A=${trunc(show(d.a))}  B=${trunc(show(d.b))}`);
    if (step.diffs.length > shown.length) console.log(`       ... ${step.diffs.length - shown.length} more differences`);
  }
}

const trunc = (s, n = 220) => (s.length > n ? `${s.slice(0, n)}...` : s);
