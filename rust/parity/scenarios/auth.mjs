// Authentication: login, refresh rotation + reuse detection, switch-group, logout, bearer parsing, session_ver.
const V1 = '/api/v1';
const PW = 'Rahasia123!';
const login = (email, extra = {}) => ({ email, password: PW, ...extra });

const post = (path, body, extra = {}) => ({ method: 'POST', path: `${V1}${path}`, body, ...extra });

export default [
  {
    name: 'auth/login',
    order: 10,
    async run(t) {
      const ok = (label, email, extra, spec = {}) => t.step(label, post('/auth/login', login(email, extra), { expect: 200, ...spec }));

      await ok('admin logs in (default group)', 'admin@alpha.test');
      await ok('email is case-insensitive', 'ADMIN@ALPHA.TEST');
      await ok('mixed-case stored email is returned lowercased', 'mixed.case@alpha.test');
      await ok('guard login', 'guard1@alpha.test');
      await ok('plain user login', 'resident@alpha.test');
      await ok('password is trimmed', 'admin@alpha.test', { password: `   ${PW}   ` });
      await ok('requested group (alpha) for admin', 'admin@alpha.test', { group_uid: 'grp-alpha' });
      await ok('requested group (beta, role user) for admin', 'admin@alpha.test', { group_uid: 'grp-beta' });
      await ok('empty group_uid falls back to default group', 'admin@alpha.test', { group_uid: '' });
      await ok('whitespace group_uid falls back to default group', 'admin@alpha.test', { group_uid: '   ' });
      await ok('multi-group user gets first JOINED group', 'multi@test.id');
      await ok('multi-group user picks beta', 'multi@test.id', { group_uid: 'grp-beta' });
      await ok('group features are split and cleaned (beta)', 'guard@beta.test');
      await ok('lowercase membership category is upper-cased', 'guard2@alpha.test');
      await ok('group_logo "N/A" becomes null (gamma)', 'login1@gamma.test');
      await ok('extra body fields are ignored', 'admin@alpha.test', { foo: 'bar', nested: { a: 1 } });

      const bad = (label, body, status, spec = {}) => t.step(label, post('/auth/login', body, { expect: status, ...spec }));
      const ip1 = t.ipFor(1);
      await bad('wrong password', login('admin@alpha.test', { password: 'salah' }), 401, { ip: ip1 });
      await bad('unknown email', login('tidak.ada@alpha.test'), 401, { ip: ip1 });
      await bad('user whose name is N/A is treated as not found', login('ghost@alpha.test'), 401, { ip: ip1 });
      await bad('password is case-sensitive', login('admin@alpha.test', { password: PW.toUpperCase() }), 401, { ip: ip1 });
      await bad('password only whitespace -> 401 not 422', login('admin@alpha.test', { password: '   ' }), 401, { ip: ip1 });
      await bad('requested group the user is not in', login('guard1@alpha.test', { group_uid: 'grp-beta' }), 403, { ip: ip1 });
      await bad('requested group unknown', login('admin@alpha.test', { group_uid: 'grp-tidak-ada' }), 403, { ip: ip1 });
      await bad('requested group where membership is JOIN_WAIT', login('admin@alpha.test', { group_uid: 'grp-gamma' }), 403, { ip: ip1 });
      await bad('user without any group', login('nogroup@test.id'), 403, { ip: ip1 });
      await bad('user only pending (JOIN_WAIT)', login('pending@alpha.test'), 403, { ip: ip1 });

      const ip2 = t.ipFor(2);
      await bad('email missing', { password: PW }, 422, { ip: ip2 });
      await bad('password missing', { email: 'admin@alpha.test' }, 422, { ip: ip2 });
      await bad('empty body object', {}, 422, { ip: ip2 });
      await bad('empty password', login('admin@alpha.test', { password: '' }), 422, { ip: ip2 });
      await bad('email not an email', login('bukan-email'), 422, { ip: ip2 });
      await bad('email with surrounding spaces fails the format check', login(' admin@alpha.test '), 422, { ip: ip2 });
      await bad('email wrong type', { email: 123, password: PW }, 422, { ip: ip2 });
      await bad('password wrong type', { email: 'admin@alpha.test', password: 123 }, 422, { ip: ip2 });
      await bad('group_uid wrong type', login('admin@alpha.test', { group_uid: 5 }), 422, { ip: ip2 });
      await bad('null email', { email: null, password: PW }, 422, { ip: ip2 });
      await bad('body is an array', [], 422, { ip: ip2 });
      await bad('body is a JSON string', 'teks', 422, { ip: ip2 });
      await bad('body is null', null, 422, { ip: ip2 });
      await bad('very long password', login('admin@alpha.test', { password: 'x'.repeat(5000) }), 401, { ip: ip2 });

      await t.step('malformed JSON', post('/auth/login', undefined, { rawBody: '{"email": "a@b.c", ', ip: ip2, expect: 400 }));
      await t.step('empty body with json content-type', { method: 'POST', path: `${V1}/auth/login`, rawBody: '', ip: ip2 });
      await t.step('no body at all', { method: 'POST', path: `${V1}/auth/login`, ip: ip2 });
      await t.step('text/plain content type', { method: 'POST', path: `${V1}/auth/login`, rawBody: JSON.stringify(login('admin@alpha.test')), headers: { 'content-type': 'text/plain' }, ip: ip2 });
      await t.step('form-urlencoded content type', { method: 'POST', path: `${V1}/auth/login`, rawBody: 'email=admin%40alpha.test&password=x', headers: { 'content-type': 'application/x-www-form-urlencoded' }, ip: ip2 });
      await t.step('GET on login -> 404', { path: `${V1}/auth/login`, ip: ip2 });
      await t.step('login ignores a bearer token', post('/auth/login', login('guard1@alpha.test'), { token: 'garbage', ip: ip2 }));
    },
  },
  {
    name: 'auth/login-lockout',
    order: 10,
    async run(t) {
      const ip = t.ipFor(1);
      const wrong = (label, email = 'lock1@gamma.test') =>
        t.step(label, post('/auth/login', { email, password: 'salah' }, { ip }));
      await wrong('fail 1');
      await wrong('fail 2');
      await wrong('fail 3 (uppercase email shares the counter)', 'LOCK1@GAMMA.TEST');
      await wrong('fail 4');
      await wrong('fail 5 locks the account but still answers 401');
      await wrong('6th attempt is locked (429 + Retry-After)');
      await t.step('correct password while locked is still 429', post('/auth/login', login('lock1@gamma.test'), { ip, expect: 429 }));
      await t.step('lock is per email: other user logs in', post('/auth/login', login('lock3@gamma.test'), { ip, expect: 200 }));
      await t.step('lock applies to the uppercase spelling too', post('/auth/login', login('LOCK1@gamma.test'), { ip, expect: 429 }));
      await t.step('lock applies from another ip', post('/auth/login', login('lock1@gamma.test'), { ip: t.ipFor(2), expect: 429 }));

      // success resets the failure counter
      const ip3 = t.ipFor(3);
      for (let i = 1; i <= 4; i++) await t.step(`lock2 fail ${i}`, post('/auth/login', { email: 'lock2@gamma.test', password: 'salah' }, { ip: ip3 }));
      await t.step('lock2 succeeds on the 5th try', post('/auth/login', login('lock2@gamma.test'), { ip: ip3, expect: 200 }));
      for (let i = 1; i <= 4; i++) await t.step(`lock2 fail again ${i}`, post('/auth/login', { email: 'lock2@gamma.test', password: 'salah' }, { ip: ip3 }));
      await t.step('counter was reset so lock2 is not locked yet', post('/auth/login', login('lock2@gamma.test'), { ip: ip3, expect: 200 }));

      // unknown emails also count against the email key and the ip
      const ip4 = t.ipFor(4);
      for (let i = 1; i <= 5; i++) await t.step(`unknown email fail ${i}`, post('/auth/login', { email: 'hantu@gamma.test', password: 'x' }, { ip: ip4 }));
      await t.step('unknown email is locked too (no enumeration)', post('/auth/login', { email: 'hantu@gamma.test', password: 'x' }, { ip: ip4, expect: 429 }));
    },
  },
  {
    name: 'auth/login-ip-throttle',
    order: 10,
    async run(t) {
      const ip = t.ipFor(5);
      for (let i = 1; i <= 10; i++) {
        await t.step(`failed attempt ${i} (distinct unknown emails)`, post('/auth/login', { email: `acak${i}@gamma.test`, password: 'x' }, { ip, expect: 401 }));
      }
      await t.step('11th attempt from this ip is throttled even with valid credentials', post('/auth/login', login('iplimit@gamma.test'), { ip, expect: 429 }));
      await t.step('throttle does not leak to another ip', post('/auth/login', login('iplimit@gamma.test'), { ip: t.ipFor(6), expect: 200 }));
      await t.step('invalid body is rejected before the throttle', post('/auth/login', {}, { ip, expect: 422 }));
    },
  },
  {
    name: 'auth/refresh',
    order: 11,
    async run(t) {
      const ip = t.ipFor(1);
      await t.step('login', post('/auth/login', login('refresh1@gamma.test'), {
        ip, expect: 200, capture: { rt1: 'data.tokens.refresh_token', at1: 'data.tokens.access_token' },
      }));
      await t.step('refresh #1 rotates the token', post('/auth/refresh', undefined, {
        ip, expect: 200, body: (v) => ({ refresh_token: v.rt1 }), capture: { rt2: 'data.refresh_token', at2: 'data.access_token' },
      }));
      await t.step('old access token still works after refresh', {
        path: `${V1}/groups/members`, token: (v) => `raw:${v.at1}`, ip, expect: 200,
      });
      await t.step('new access token works', { path: `${V1}/groups/members`, token: (v) => `raw:${v.at2}`, ip, expect: 200 });
      await t.step('replaying rt1 inside the 15s grace window returns the same tokens', post('/auth/refresh', undefined, {
        ip, expect: 200, body: (v) => ({ refresh_token: v.rt1 }),
      }));
      await t.step('refresh with the new token rotates again', post('/auth/refresh', undefined, {
        ip, expect: 200, body: (v) => ({ refresh_token: v.rt2 }), capture: { rt3: 'data.refresh_token' },
      }));
      await t.step('refresh token is trimmed', post('/auth/refresh', undefined, {
        ip, expect: 200, body: (v) => ({ refresh_token: `  ${v.rt3}  ` }), capture: { rt4: 'data.refresh_token' },
      }));

      const bad = (label, body, status) => t.step(label, post('/auth/refresh', body, { ip, expect: status }));
      await bad('unknown refresh token', { refresh_token: 'a'.repeat(64) }, 401);
      await bad('garbage refresh token', { refresh_token: 'bukan-token' }, 401);
      await bad('whitespace-only refresh token -> 400 from the use case', { refresh_token: '   ' }, 400);
      await bad('empty refresh token -> 422', { refresh_token: '' }, 422);
      await bad('missing refresh_token', {}, 422);
      await bad('wrong type', { refresh_token: 12345 }, 422);
      await bad('body is an array', [], 422);
      await t.step('malformed json', post('/auth/refresh', undefined, { ip, rawBody: '{', expect: 400 }));
      await t.step('refresh ignores any Authorization header', post('/auth/refresh', undefined, { ip, token: 'expired', body: (v) => ({ refresh_token: v.rt4 }), expect: 200 }));

      // reuse detection: the grace window (15s) must have passed before the tombstone check is reached
      await t.step('login for reuse test', post('/auth/login', login('refresh2@gamma.test'), {
        ip, expect: 200, capture: { r1: 'data.tokens.refresh_token' },
      }));
      await t.step('rotate once', post('/auth/refresh', undefined, {
        ip, expect: 200, body: (v) => ({ refresh_token: v.r1 }), capture: { r2: 'data.refresh_token' },
      }));
      await t.sleep(16000);
      await t.step('replaying the rotated token after the grace window is detected as reuse', post('/auth/refresh', undefined, {
        ip, expect: 401, body: (v) => ({ refresh_token: v.r1 }),
      }));
      await t.step('the whole family is revoked: the newest token is dead', post('/auth/refresh', undefined, {
        ip, expect: 401, body: (v) => ({ refresh_token: v.r2 }),
      }));
      await t.step('login again after revocation works', post('/auth/login', login('refresh2@gamma.test'), { ip, expect: 200 }));
    },
  },
  {
    name: 'auth/refresh-after-logout',
    order: 11,
    async run(t) {
      const ip = t.ipFor(1);
      await t.step('login', post('/auth/login', login('refresh3@gamma.test'), {
        ip, expect: 200, capture: { rt: 'data.tokens.refresh_token', at: 'data.tokens.access_token' },
      }));
      await t.step('logout revokes the refresh token', post('/auth/logout', undefined, {
        ip, expect: 200, token: (v) => `raw:${v.at}`, body: (v) => ({ refresh_token: v.rt }),
      }));
      await t.step('refresh with the revoked token', post('/auth/refresh', undefined, { ip, expect: 401, body: (v) => ({ refresh_token: v.rt }) }));
      await t.step('the old access token is stale after logout', { path: `${V1}/groups/members`, ip, expect: 401, token: (v) => `raw:${v.at}` });
      await t.step('login after logout gets a token with the bumped session_ver', post('/auth/login', login('refresh3@gamma.test'), {
        ip, expect: 200, capture: { at2: 'data.tokens.access_token' },
      }));
      await t.step('new token works', { path: `${V1}/groups/members`, ip, expect: 200, token: (v) => `raw:${v.at2}` });
    },
  },
  {
    name: 'auth/switch-group',
    order: 12,
    async run(t) {
      const ip = t.ipFor(1);
      await t.step('login (default group gamma, role ADMIN)', post('/auth/login', login('switch1@gamma.test'), {
        ip, expect: 200, capture: { at: 'data.tokens.access_token', rt: 'data.tokens.refresh_token' },
      }));
      const sw = (label, body, status, extra = {}) =>
        t.step(label, post('/auth/switch-group', body, { ip, token: (v) => `raw:${v.at}`, expect: status, ...extra }));
      await sw('switch to alpha (role GUARD)', { group_uid: 'grp-alpha' }, 200, { capture: { at2: 'data.tokens.access_token', rt2: 'data.tokens.refresh_token' } });
      await t.step('new token carries the alpha group', { path: `${V1}/guests`, ip, token: (v) => `raw:${v.at2}`, query: { limit: 2 }, expect: 200 });
      await t.step('refresh of the switched session keeps the alpha group', post('/auth/refresh', undefined, {
        ip, expect: 200, body: (v) => ({ refresh_token: v.rt2 }),
      }));
      await sw('switch to a group with JOIN_WAIT membership', { group_uid: 'grp-beta' }, 403);
      await sw('switch to a group the user is not a member of', { group_uid: 'grp-tidak-ada' }, 403);
      await sw('empty group_uid -> 422', { group_uid: '' }, 422);
      await sw('whitespace group_uid -> 400', { group_uid: '   ' }, 400);
      await sw('missing group_uid -> 422', {}, 422);
      await sw('group_uid wrong type', { group_uid: 7 }, 422);
      await sw('switch back to gamma', { group_uid: 'grp-gamma' }, 200);
      await t.step('no token -> 401', post('/auth/switch-group', { group_uid: 'grp-alpha' }, { ip, expect: 401 }));
      await t.step('no token and invalid body -> validation wins over auth', post('/auth/switch-group', {}, { ip }));
      await t.step('garbage token -> 401', post('/auth/switch-group', { group_uid: 'grp-alpha' }, { ip, token: 'garbage', expect: 401 }));
      await t.step('stale session -> 401 AUTH_SESSION_STALE', post('/auth/switch-group', { group_uid: 'grp-alpha' }, { ip, token: 'stale_user', expect: 401 }));
      await t.step('token for a user id that is not in tkuser -> 401 user not found', post('/auth/switch-group', { group_uid: 'grp-alpha' }, { ip, token: 'newer_than_redis' }));
      await t.step('user without groups cannot switch', post('/auth/switch-group', { group_uid: 'grp-alpha' }, { ip, token: 'user_nogroup', expect: 403 }));
    },
  },
  {
    name: 'auth/logout',
    order: 12,
    async run(t) {
      const ip = t.ipFor(1);
      await t.step('login', post('/auth/login', login('logout1@gamma.test'), {
        ip, expect: 200, capture: { at: 'data.tokens.access_token', rt: 'data.tokens.refresh_token' },
      }));
      await t.step('logout with token + refresh token', post('/auth/logout', undefined, {
        ip, expect: 200, token: (v) => `raw:${v.at}`, body: (v) => ({ refresh_token: v.rt }),
      }));
      await t.step('access token is stale afterwards', { path: `${V1}/groups/members`, ip, expect: 401, token: (v) => `raw:${v.at}` });
      await t.step('logout again with the stale token still answers 200', post('/auth/logout', {}, { ip, expect: 200, token: (v) => `raw:${v.at}` }));
      await t.step('logout without any token answers 200', post('/auth/logout', {}, { ip, expect: 200 }));
      await t.step('logout with garbage token and unknown refresh token', post('/auth/logout', { refresh_token: 'nope' }, { ip, token: 'garbage', expect: 200 }));
      await t.step('logout with empty refresh_token string is allowed', post('/auth/logout', { refresh_token: '' }, { ip, expect: 200 }));
      await t.step('logout with wrong type -> 422', post('/auth/logout', { refresh_token: 5 }, { ip, expect: 422 }));
      await t.step('logout with no body at all', { method: 'POST', path: `${V1}/auth/logout`, ip });
      await t.step('logout with malformed json -> 400', post('/auth/logout', undefined, { ip, rawBody: '{x', expect: 400 }));
      await t.step('logout with an array body is accepted (all-optional object)', post('/auth/logout', [], { ip, expect: 200 }));
      await t.step('login (second session)', post('/auth/login', login('logout2@gamma.test'), {
        ip, expect: 200, capture: { at: 'data.tokens.access_token', rt: 'data.tokens.refresh_token' },
      }));
      await t.step('logout with refresh token only, no bearer', post('/auth/logout', undefined, { ip, expect: 200, body: (v) => ({ refresh_token: v.rt }) }));
      await t.step('refresh token is dead but the access token is still valid (no session bump)', { path: `${V1}/groups/members`, ip, expect: 200, token: (v) => `raw:${v.at}` });
      await t.step('refresh token is dead', post('/auth/refresh', undefined, { ip, expect: 401, body: (v) => ({ refresh_token: v.rt }) }));
    },
  },
  {
    name: 'auth/bearer-and-session',
    order: 13,
    async run(t) {
      const members = (label, spec) => t.step(label, { path: `${V1}/groups/members`, ...spec });
      await members('no Authorization header -> 401', { expect: 401 });
      await members('Basic scheme', { headers: { authorization: 'Basic dXNlcjpwYXNz' }, expect: 401 });
      await members('"Bearer" with nothing after it', { headers: { authorization: 'Bearer' }, expect: 401 });
      await members('"Bearer " with only a space', { headers: { authorization: 'Bearer ' }, expect: 401 });
      await members('lowercase "bearer" scheme', { headers: (v) => ({ authorization: `bearer ${t.tokens.guard1_g1}` }), expect: 401 });
      await members('double space after Bearer is trimmed', { headers: (v) => ({ authorization: `Bearer   ${t.tokens.guard1_g1}` }), expect: 200 });
      await members('trailing whitespace is trimmed', { headers: (v) => ({ authorization: `Bearer ${t.tokens.guard1_g1}  ` }), expect: 200 });
      await members('token without scheme', { headers: (v) => ({ authorization: t.tokens.guard1_g1 }), expect: 401 });
      for (const name of ['expired', 'wrong_issuer', 'wrong_audience', 'missing_issuer', 'missing_sub', 'missing_email', 'bad_signature', 'hs256_token', 'alg_none', 'tampered_payload', 'garbage', 'two_parts', 'ver_zero', 'ver_missing']) {
        await members(`token "${name}" -> 401 AUTH_UNAUTHORIZED`, { token: name, expect: 401 });
      }
      await members('unknown kid falls back to the active key -> accepted', { token: 'unknown_kid', expect: 200 });
      await members('session_ver sent as string "1" is accepted', { token: 'ver_as_string', expect: 200 });
      await members('session_ver 1.9 is truncated to 1 and accepted', { token: 'ver_float', expect: 200 });
      await members('session_ver lower than Redis -> AUTH_SESSION_STALE', { token: 'stale_user', expect: 401 });
      await members('session_ver higher than Redis is accepted', { token: 'newer_than_redis', expect: 200 });
      await members('missing Redis key is rehydrated from the token (v3)', { token: 'rehydrate_v3', expect: 200 });
      await members('after rehydration an older version is stale', { token: 'rehydrate_v2', expect: 401 });
      await members('and the rehydrated version still works', { token: 'rehydrate_v3', expect: 200 });
      await members('role is upper-cased from the claim', { token: 'guard_lowercase_role', expect: 200 });
      await members('lowercase email in claim is preserved', { token: 'guard_upper_email', expect: 200 });
      await members('token for a user without group -> empty list', { token: 'user_nogroup', expect: 200 });

      // requireRole routes use a different 401 message than requireAuth routes
      await t.step('requireAuth route without token', { path: `${V1}/guests`, expect: 401 });
      await t.step('requireRole route without token', { method: 'POST', path: `${V1}/guests`, body: { guest_name: 'x', guest_destination: 'y', guest_type: 'Tamu Umum' }, expect: 401 });
      await t.step('requireRole route with stale token', { method: 'POST', path: `${V1}/guests`, token: 'stale_user', body: { guest_name: 'x', guest_destination: 'y', guest_type: 'Tamu Umum' }, expect: 401 });
      await t.step('requireRole route, role not allowed', { method: 'POST', path: `${V1}/guests`, token: 'resident_g1', body: { guest_name: 'x', guest_destination: 'y', guest_type: 'Tamu Umum' }, expect: 403 });
      await t.step('requireRole route, role VIEWER', { method: 'POST', path: `${V1}/guests`, token: 'viewer_role', body: { guest_name: 'x', guest_destination: 'y', guest_type: 'Tamu Umum' }, expect: 403 });
      await t.step('requireRole route, role "N/A" cleaned to null', { method: 'POST', path: `${V1}/guests`, token: 'guard_na_role', body: { guest_name: 'x', guest_destination: 'y', guest_type: 'Tamu Umum' }, expect: 403 });
    },
  },
];
