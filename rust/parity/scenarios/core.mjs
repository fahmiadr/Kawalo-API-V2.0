// Cross-cutting behaviour: health, JWKS, routing fallbacks, CORS, security headers, request ids, global rate limit.
const V1 = '/api/v1';

export default [
  {
    name: 'core/health',
    order: 1,
    async run(t) {
      await t.step('GET /health', { path: '/health', expect: 200 });
      await t.step('GET /health/ready', { path: '/health/ready', expect: 200 });
      await t.step('HEAD /health', { method: 'HEAD', path: '/health' });
      await t.step('POST /health -> 404', { method: 'POST', path: '/health', expect: 404 });
      await t.step('GET /health/ -> trailing slash', { path: '/health/' });
      await t.step('GET /health/ready/ -> trailing slash', { path: '/health/ready/' });
      await t.step('GET /HEALTH -> case sensitive', { path: '/HEALTH' });
      await t.step('query string is ignored', { path: '/health', query: { a: 1, b: 'x' } });
    },
  },
  {
    name: 'core/jwks',
    order: 1,
    async run(t) {
      await t.step('GET jwks', { path: '/.well-known/jwks.json', expect: 200 });
      await t.step('POST jwks -> 404', { method: 'POST', path: '/.well-known/jwks.json', expect: 404 });
      await t.step('jwks does not need auth, extra bearer is ignored', { path: '/.well-known/jwks.json', token: 'garbage' });
    },
  },
  {
    name: 'core/routing',
    order: 1,
    async run(t) {
      await t.step('unknown root path', { path: '/nope', expect: 404 });
      await t.step('unknown api path', { path: `${V1}/nope`, expect: 404 });
      await t.step('api root', { path: V1, expect: 404 });
      await t.step('unversioned api path', { path: '/api/guests', token: 'guard1_g1', expect: 404 });
      await t.step('v2 does not exist', { path: '/api/v2/guests', token: 'guard1_g1', expect: 404 });
      await t.step('unknown path with auth', { path: `${V1}/nope`, token: 'guard1_g1', expect: 404 });
      await t.step('wrong method on guests collection (PATCH)', { method: 'PATCH', path: `${V1}/guests`, token: 'guard1_g1' });
      await t.step('wrong method on groups/members (POST)', { method: 'POST', path: `${V1}/groups/members`, token: 'guard1_g1' });
      await t.step('wrong method on attendance (DELETE)', { method: 'DELETE', path: `${V1}/attendance`, token: 'guard1_g1' });
      await t.step('wrong method on operational (PUT)', { method: 'PUT', path: `${V1}/operational`, token: 'guard1_g1' });
      await t.step('wrong method on incidents (PUT)', { method: 'PUT', path: `${V1}/incidents`, token: 'guard1_g1' });
      await t.step('trailing slash on collection', { path: `${V1}/guests/`, token: 'guard1_g1' });
      await t.step('double slash', { path: `${V1}//guests`, token: 'guard1_g1' });
      await t.step('percent-encoded path segment', { path: `${V1}/guests/%31`, token: 'guard1_g1' });
      await t.step('very long unknown path', { path: `/${'x'.repeat(2000)}`, expect: 404 });
    },
  },
  {
    name: 'core/headers-cors-requestid',
    order: 1,
    async run(t) {
      await t.step('x-request-id generated', { path: '/health' });
      await t.step('x-request-id echoed', { path: '/health', headers: { 'x-request-id': 'parity-req-1' } });
      await t.step('x-request-id echoed on 401', { path: `${V1}/guests`, headers: { 'x-request-id': 'parity-req-2' }, expect: 401 });
      await t.step('x-request-id echoed on 404', { path: '/nope', headers: { 'x-request-id': 'parity-req-3' } });
      await t.step('x-request-id echoed on 422', { method: 'POST', path: `${V1}/auth/login`, body: {}, headers: { 'x-request-id': 'parity-req-4' } });
      await t.step('x-request-id echoed on 400 (bad json)', { method: 'POST', path: `${V1}/auth/login`, rawBody: '{bad', headers: { 'x-request-id': 'parity-req-5' } });
      await t.step('empty x-request-id header', { path: '/health', headers: { 'x-request-id': '' } });
      await t.step('CORS simple request with Origin', { path: '/health', headers: { origin: 'https://app.example.test' } });
      await t.step('CORS preflight POST', {
        method: 'OPTIONS',
        path: `${V1}/guests`,
        headers: { origin: 'https://app.example.test', 'access-control-request-method': 'POST', 'access-control-request-headers': 'authorization,content-type' },
      });
      await t.step('CORS preflight unknown path', {
        method: 'OPTIONS',
        path: '/nope',
        headers: { origin: 'https://app.example.test', 'access-control-request-method': 'GET' },
      });
      await t.step('OPTIONS without Origin', { method: 'OPTIONS', path: `${V1}/guests` });
      await t.step('CORS on 401 error response', { path: `${V1}/guests`, headers: { origin: 'https://app.example.test' } });
      await t.step('security headers on SSE-less 200 json', { path: `${V1}/groups/members`, token: 'guard1_g1', expect: 200 });
      await t.step('security headers on 500-free 404', { path: `${V1}/guests/999999`, token: 'guard1_g1', expect: 404 });
    },
  },
  {
    name: 'core/rate-limit-global',
    order: 99,
    async run(t) {
      // The servers are started with RATE_LIMIT_MAX=300 per 60s per client ip (see services.sh).
      const ip = t.ip;
      await t.compute('300 requests pass, 301st is limited', async (srv) => {
        const statuses = [];
        let first429 = null;
        for (let i = 0; i < 302; i++) {
          const r = await srv.call({ path: '/health', ip });
          statuses.push(r.status);
          if (r.status === 429 && !first429) first429 = r.snapshot;
        }
        const ok = statuses.filter((s) => s === 200).length;
        const limited = statuses.filter((s) => s === 429).length;
        return { ok, limited, firstLimitedAt: statuses.indexOf(429) + 1, first429 };
      });
      await t.step('other ip is not affected', { path: '/health', ip: ip.replace(/^(\d+\.\d+\.)(\d+)/, (_m, p, n) => `${p}${(Number(n) + 1) % 256}`) });
      await t.step('limit also applies to api routes', { path: `${V1}/groups/members`, token: 'guard1_g1', ip });
      // Deliberately absent: a rate-limited request to an UNKNOWN route. The TS server answers it once and
      // then crashes the whole process on the next one (elysia-rate-limit re-clones a consumed Response in
      // its onError hook). The intended behaviour is the same 429 body as above.
    },
  },
];
