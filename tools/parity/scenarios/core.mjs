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
    name: 'core/trailing-slash',
    order: 1,
    async run(t) {
      const paths = ['/health/', '/health//', '//health', '/health/?a=1', '/health?a=1/', '/', '//', '/health/ready/', '/.well-known/jwks.json/', '/health;a=1', '/%68ealth', '/health/%2F', '/health#x'];
      for (const path of paths) await t.step(`GET ${path}`, { path });
      for (const path of ['/groups/members/', '/guests/', '/guests//', '/guests/master-types/', '/guests/master-types//', '/guests/1/', '/guests/1//', '/information/', '/incidents/', '/attendance/', '/operational/', '/operational/?limit=1']) {
        await t.step(`GET ${V1}${path}`, { path: `${V1}${path}`, token: 'admin_g1' });
      }
      await t.step('POST login with trailing slash', { method: 'POST', path: `${V1}/auth/login/`, body: { email: 'nobody@x.test', password: 'x' } });
      await t.step('POST /health/ -> 404', { method: 'POST', path: '/health/' });
      await t.step('OPTIONS /health/', { method: 'OPTIONS', path: '/health/', headers: { origin: 'https://app.example.test', 'access-control-request-method': 'GET' } });
    },
  },
  {
    name: 'core/body-parsers',
    order: 2,
    async run(t) {
      // Elysia picks the parser from the 13th character of the content type (before any `;`), without trimming or case folding.
      const login = (label, contentType, rawBody, extra = {}) =>
        t.step(`login: ${label}`, { method: 'POST', path: `${V1}/auth/login`, rawBody, headers: contentType === undefined ? { 'content-type': '' } : { 'content-type': contentType }, ...extra });
      const creds = 'email=nobody%40x.test&password=x';
      const json = '{"email":"nobody@x.test","password":"x"}';
      await login('form', 'application/x-www-form-urlencoded', creds);
      await login('form with charset', 'application/x-www-form-urlencoded; charset=utf-8', creds);
      await login('form with a space before the parameter', 'application/x-www-form-urlencoded ; charset=utf-8', creds);
      await login('form, plus is a space', 'application/x-www-form-urlencoded', 'email=nobody%40x.test&password=a+b');
      await login('form, repeated password is an array -> 422', 'application/x-www-form-urlencoded', `${creds}&password=y`);
      await login('form, repeated email is an array -> 422', 'application/x-www-form-urlencoded', `email=a%40b.co&email=c%40d.co&password=x`);
      await login('form, missing password -> 422', 'application/x-www-form-urlencoded', 'email=nobody%40x.test');
      await login('form, key without value -> 422', 'application/x-www-form-urlencoded', 'email=nobody%40x.test&password');
      await login('form, empty body -> 422', 'application/x-www-form-urlencoded', '');
      await login('form, percent-encoded key', 'application/x-www-form-urlencoded', 'em%61il=nobody%40x.test&password=x');
      await login('form, empty key and unknown keys are ignored', 'application/x-www-form-urlencoded', `=1&zzz=2&&${creds}`);
      await login('form, bad percent sequence', 'application/x-www-form-urlencoded', 'email=nobody%40x.test&password=%ZZ');
      await login('uppercase content type is not a form -> 422', 'APPLICATION/X-WWW-FORM-URLENCODED', creds);
      await login('application/xml is read as a form', 'application/xml', creds);
      await login('JSON body with a form content type -> 422', 'application/x-www-form-urlencoded', json);
      await login('text/plain with JSON text -> 422', 'text/plain', json);
      await login('text/plain empty -> 422', 'text/plain', '');
      await login('text/html -> 422', 'text/html', json);
      await login('octet-stream -> 422', 'application/octet-stream', json);
      await login('no content type -> 422', undefined, json);
      await login('image/png -> 422', 'image/png', json);
      await login('uppercase JSON content type -> 422', 'APPLICATION/JSON', json);
      await login('vendor +json -> 422', 'application/vnd.api+json', json);
      await login('json with charset', 'application/json; charset=utf-8', json);
      await login('json with odd charset', 'application/json;charset=latin1', json);
      await login('application/jsonx is read as JSON', 'application/jsonx', json);
      await login('text/json is text -> 422', 'text/json', json);
      await login('empty JSON -> 400', 'application/json', '');
      await login('whitespace JSON -> 400', 'application/json', '   ');
      await login('JSON null -> 422', 'application/json', 'null');
      await login('JSON array -> 422', 'application/json', '[]');
      await login('JSON trailing garbage -> 400', 'application/json', `${json} x`);

      const part = (name, value, file) =>
        `--XX\r\nContent-Disposition: form-data; name="${name}"${file ? `; filename="${file}"\r\nContent-Type: application/octet-stream` : ''}\r\n\r\n${value}\r\n`;
      const mp = (...parts) => `${parts.join('')}--XX--\r\n`;
      const MP = 'multipart/form-data; boundary=XX';
      await login('multipart', MP, mp(part('email', 'nobody@x.test'), part('password', 'x')));
      await login('multipart, missing password -> 422', MP, mp(part('email', 'nobody@x.test')));
      await login('multipart, file instead of text -> 422', MP, mp(part('email', 'nobody@x.test'), part('password', 'x', 'p.txt')));
      await login('multipart, repeated key -> 422', MP, mp(part('email', 'nobody@x.test'), part('password', 'x'), part('password', 'y')));
      await login('multipart, JSON-looking text for a string -> 422', MP, mp(part('email', 'nobody@x.test'), part('password', '{"a":1}')));
      await login('multipart, broken body -> 400', MP, 'garbage');
      await login('multipart without boundary -> 400', 'multipart/form-data', mp(part('email', 'a')));
      await login('multipart, empty -> 422 or 400', MP, '');

      const incident = (label, contentType, rawBody, expect) =>
        t.step(`incident: ${label}`, { method: 'POST', path: `${V1}/incidents`, token: 'guard1_g1', rawBody, headers: { 'content-type': contentType }, ...(expect ? { expect } : {}), ignore: ['data.id', 'data.created_at', 'data.updated_at'] });
      const FORM = 'application/x-www-form-urlencoded';
      await incident('form with title', FORM, 'title=Form+title', 201);
      await incident('form, photos repeated', FORM, 'title=Form&photos=a&photos=b', 201);
      await incident('form, photos once is a string -> 422', FORM, 'title=Form&photos=a', 422);
      await incident('form, photos[] is another key', FORM, 'title=Form&photos%5B%5D=a', 201);
      await incident('form, glat is text -> 422', FORM, 'title=Form&glat=1.5', 422);
      await incident('form, empty description', FORM, 'title=Form&description=', 201);
      await incident('form, category out of enum', FORM, 'title=Form&category=zzz');
      await incident('form, repeated title -> 422', FORM, 'title=A&title=B', 422);
      await incident('form, empty title -> 422', FORM, 'title=', 422);
      await incident('form, five photos -> 422', FORM, 'title=Form&photos=a&photos=b&photos=c&photos=d&photos=e', 422);
      await incident('multipart with title', MP, mp(part('title', 'Multi')), 201);
      await incident('multipart, photos repeated', MP, mp(part('title', 'Multi'), part('photos', 'a'), part('photos', 'b')), 201);
      await incident('multipart, photos as JSON text', MP, mp(part('title', 'Multi'), part('photos', '["a","b"]')), 201);
      await incident('multipart, photos once -> 422', MP, mp(part('title', 'Multi'), part('photos', 'a')), 422);
      await incident('multipart, glat is text -> 422', MP, mp(part('title', 'Multi'), part('glat', '1.5')), 422);
      await incident('multipart, extra file part is ignored', MP, mp(part('title', 'Multi'), part('upload', 'x', 'u.txt')), 201);
      await incident('multipart, title as file -> 422', MP, mp(part('title', 'x', 't.txt')), 422);
      await incident('text/plain -> 422', 'text/plain', 'title=Plain', 422);

      // All-optional schema: an opaque body counts as an empty object and succeeds.
      const patch = (label, contentType, rawBody) =>
        t.step(`guest PATCH: ${label}`, { method: 'PATCH', path: `${V1}/guests/1`, token: 'admin_g1', rawBody, headers: { 'content-type': contentType } });
      await patch('text/plain', 'text/plain', 'x');
      await patch('text/plain empty', 'text/plain', '');
      await patch('octet-stream', 'application/octet-stream', 'x');
      await patch('form', FORM, 'guest_name=Dari+Form');
      await patch('form, empty', FORM, '');
      await patch('multipart', MP, mp(part('guest_name', 'Dari Multipart')));
      await patch('multipart, JSON object text for a string -> 422', MP, mp(part('guest_purpose', '{"a":1}')));
      await patch('unknown content type -> 422', 'image/png', 'x');
      await t.step('guest PATCH: no content type and no body -> 422', { method: 'PATCH', path: `${V1}/guests/1`, token: 'admin_g1' });
      await t.step('guest PATCH: no body, JSON content type -> 400', { method: 'PATCH', path: `${V1}/guests/1`, token: 'admin_g1', headers: { 'content-type': 'application/json' } });
      await t.step('validation beats auth on a form body', { method: 'POST', path: `${V1}/incidents`, rawBody: 'title=', headers: { 'content-type': FORM } });
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
