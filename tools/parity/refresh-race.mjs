// Concurrent refresh of ONE refresh token: the outcome multiset must match between servers.
//   node refresh-race.mjs --a http://127.0.0.1:3410 --b http://127.0.0.1:3411 [--trials 10] [--parallel 8]
const arg = (n, d) => {
  const i = process.argv.indexOf(`--${n}`);
  return i > 0 ? process.argv[i + 1] : d;
};
const trials = Number(arg('trials', 10));
const parallel = Number(arg('parallel', 8));
const post = (base, path, body, ip) =>
  fetch(base + path, { method: 'POST', headers: { 'content-type': 'application/json', 'x-forwarded-for': ip }, body: JSON.stringify(body) }).then(async (r) => ({ status: r.status, json: await r.json() }));

async function race(base, ip) {
  const outcomes = {};
  for (let i = 0; i < trials; i++) {
    const login = await post(base, '/api/v1/auth/login', { email: 'login2@gamma.test', password: 'Rahasia123!' }, ip);
    const refresh = login.json.data.tokens.refresh_token;
    const results = await Promise.all(Array.from({ length: parallel }, () => post(base, '/api/v1/auth/refresh', { refresh_token: refresh }, ip)));
    const ok = results.filter((r) => r.status === 200);
    const sameTokens = new Set(ok.map((r) => r.json.data.refresh_token)).size;
    const key = JSON.stringify({
      statuses: [...new Set(results.map((r) => `${r.status}:${r.json.error_code ?? 'ok'}`))].sort(),
      // 200s are either one winner or the winner plus grace replays carrying identical tokens
      distinctRefreshTokens: sameTokens,
    });
    outcomes[key] = (outcomes[key] ?? 0) + 1;
    // The family must still be usable or cleanly revoked, never half-open: a follow-up with the new token works.
    if (ok[0]) {
      const next = await post(base, '/api/v1/auth/refresh', { refresh_token: ok[0].json.data.refresh_token }, ip);
      const k = `follow-up ${next.status}:${next.json.error_code ?? 'ok'}`;
      outcomes[k] = (outcomes[k] ?? 0) + 1;
    }
  }
  return outcomes;
}
// a fresh client ip per run keeps the global rate limit (300/min) and the login throttle out of the way
const octet = 1 + Math.floor(Math.random() * 250);
const [a, b] = await Promise.all([race(arg('a'), `203.0.113.${octet}`), race(arg('b'), `203.0.113.${octet}`)]);
console.log('A', JSON.stringify(a, null, 1));
console.log('B', JSON.stringify(b, null, 1));
const same = JSON.stringify(Object.keys(a).sort()) === JSON.stringify(Object.keys(b).sort());
console.log(same ? 'SAME outcome shapes' : 'DIFFERENT outcome shapes');
process.exit(same ? 0 : 1);
