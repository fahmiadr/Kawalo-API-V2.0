// Normalization: only what is genuinely non-deterministic is rewritten. Everything else is compared literally.
import { createPublicKey, verify } from 'node:crypto';

const V7 = '[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}';
const V4 = '[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}';
const PREFIXED_UID = new RegExp(`\\b(inc|rpl|guest|feed)_(${V7})\\b`, 'g');
const BARE_V7 = new RegExp(`\\b${V7}\\b`, 'g');
const BARE_V4 = new RegExp(`\\b${V4}\\b`, 'g');
const ISO_TS = /^\d{4}-\d\d-\d\dT\d\d:\d\d:\d\d(\.\d{1,3})?Z$/;
const JWT = /^eyJ[\w-]+\.eyJ[\w-]+\.[\w-]*$/;
const HEX64 = /^[0-9a-f]{64}$/;

// Keys whose numeric value is a clock/uptime/latency reading.
const VOLATILE_NUMBER_KEYS = new Set(['uptime', 'latencyMs']);
// Error text from the DB driver or the runtime embeds engine-specific details; compared only by "is non-empty".
const WINDOW_MS = 10 * 60 * 1000;

/** Per-server, per-scenario state so placeholders are numbered by order of first appearance. */
export class Normalizer {
  constructor({ jwks = null, runStartMs = Date.now() } = {}) {
    this.maps = new Map();
    this.jwks = jwks;
    this.runStartMs = runStartMs;
  }

  #index(kind, value) {
    let m = this.maps.get(kind);
    if (!m) this.maps.set(kind, (m = new Map()));
    if (!m.has(value)) m.set(value, m.size + 1);
    return m.get(value);
  }

  isNow(ms) {
    return Math.abs(ms - Date.now()) <= WINDOW_MS || Math.abs(ms - this.runStartMs) <= WINDOW_MS;
  }

  timestamp(s) {
    const ms = Date.parse(s);
    return Number.isNaN(ms) ? s : this.isNow(ms) ? '<now>' : s;
  }

  jwt(token) {
    const [h, p, s] = token.split('.');
    let header;
    let claims;
    try {
      header = JSON.parse(Buffer.from(h, 'base64url').toString());
      claims = JSON.parse(Buffer.from(p, 'base64url').toString());
    } catch {
      return `<jwt#${this.#index('jwt', token)}:unparseable>`;
    }
    let sig = 'unchecked';
    const jwk = this.jwks?.keys?.find((k) => k.kid === header.kid) ?? this.jwks?.keys?.[0];
    if (jwk && header.alg === 'RS256') {
      try {
        const key = createPublicKey({ key: jwk, format: 'jwk' });
        sig = verify('RSA-SHA256', Buffer.from(`${h}.${p}`), key, Buffer.from(s, 'base64url')) ? 'valid' : 'INVALID';
      } catch {
        sig = 'INVALID';
      }
    }
    const { iat, exp, ...rest } = claims;
    return {
      $jwt: this.#index('jwt', token),
      sig,
      header,
      claims: {
        ...rest,
        iat: typeof iat === 'number' ? (this.isNow(iat * 1000) ? '<now>' : iat) : iat,
        ttl: typeof iat === 'number' && typeof exp === 'number' ? exp - iat : exp,
      },
    };
  }

  string(s) {
    if (JWT.test(s)) return this.jwt(s);
    if (HEX64.test(s)) return `<hex64#${this.#index('hex64', s)}>`;
    if (ISO_TS.test(s)) return this.timestamp(s);
    return s
      .replace(PREFIXED_UID, (_m, prefix, uuid) => `<${prefix}#${this.#index(prefix, uuid)}>`)
      .replace(BARE_V4, (m) => `<uuid4#${this.#index('uuid4', m)}>`)
      .replace(BARE_V7, (m) => `<uuid7#${this.#index('uuid7', m)}>`);
  }

  /** Walks a parsed JSON value; object keys are visited in sorted order so numbering ignores key order. */
  value(v, key = '') {
    if (typeof v === 'string') return this.string(v);
    if (typeof v === 'number') return VOLATILE_NUMBER_KEYS.has(key) ? '<num>' : v;
    if (Array.isArray(v)) return v.map((x) => this.value(x, key));
    if (v && typeof v === 'object') {
      const out = {};
      for (const k of Object.keys(v).sort()) out[k] = this.value(v[k], k);
      return out;
    }
    return v;
  }
}

/** Structural diff; returns [{path, a, b}] with arrays compared by index and objects by key (order-insensitive). */
export function diff(a, b, path = '$', out = []) {
  if (Object.is(a, b)) return out;
  const ta = kind(a);
  const tb = kind(b);
  if (ta !== tb) {
    out.push({ path, a, b });
  } else if (ta === 'array') {
    if (a.length !== b.length) out.push({ path: `${path}.length`, a: a.length, b: b.length });
    for (let i = 0; i < Math.max(a.length, b.length); i++) {
      if (i >= a.length) out.push({ path: `${path}[${i}]`, a: undefined, b: b[i] });
      else if (i >= b.length) out.push({ path: `${path}[${i}]`, a: a[i], b: undefined });
      else diff(a[i], b[i], `${path}[${i}]`, out);
    }
  } else if (ta === 'object') {
    for (const k of new Set([...Object.keys(a), ...Object.keys(b)])) {
      if (!(k in a)) out.push({ path: `${path}.${k}`, a: undefined, b: b[k] });
      else if (!(k in b)) out.push({ path: `${path}.${k}`, a: a[k], b: undefined });
      else diff(a[k], b[k], `${path}.${k}`, out);
    }
  } else {
    out.push({ path, a, b });
  }
  return out;
}

function kind(v) {
  if (v === null) return 'null';
  if (Array.isArray(v)) return 'array';
  return typeof v;
}

export const show = (v) => (v === undefined ? '<absent>' : JSON.stringify(v));
