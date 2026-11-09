// Issues long-lived bench tokens with the repo's own TokenService (reads certs/ programmatically, never prints them).
// usage (from the API root): REDIS_URL=... JWT_EXPIRES_IN=604800 npx tsx rust/bench/mktokens.mts [out.json]
import { chmodSync, writeFileSync } from 'node:fs';
import { RsaKeyManager } from '@core/security/rsa-key.manager';
import { SessionVersionService } from '@core/security/session-version.service';
import { TokenService } from '@core/security/token.service';
import { redis } from '@core/database/redis.service';

const out = process.argv[2] ?? '/tmp/kw-bench-tokens.json';
const GROUP = 'bench-g1';
const PTT_USERS = 300;

async function tokenFor(sub: string, email: string, role: 'ADMIN' | 'GUARD') {
  const session_ver = await SessionVersionService.getOrInitVersion(sub);
  const { token } = await TokenService.signAccessToken({
    sub, email, name: email.split('@')[0], role, group_uid: GROUP,
    allow_admin: role === 'ADMIN', allow_patrol: true, allow_mon_patrol: false, session_ver,
  });
  return token;
}

await RsaKeyManager.init();
// Seeded users: id 1 is GUARD, id 10 is ADMIN (see seed.mjs), both in bench-g1.
const guard = await tokenFor('1', 'bench-user-1@bench.test', 'GUARD');
const admin = await tokenFor('10', 'bench-user-10@bench.test', 'ADMIN');
const ptt: string[] = [];
for (let i = 1; i <= PTT_USERS; i++) ptt.push(await tokenFor(`ptt-${i}`, `ptt-${i}@bench.test`, 'GUARD'));

writeFileSync(out, JSON.stringify({
  group: GROUP, guard, admin, ptt,
  login: { email: 'bench-user-1@bench.test', password: 'bench123' },
}));
chmodSync(out, 0o600);
console.log(`tokens written: ${out} (guard, admin, ${PTT_USERS} ptt)`);
await redis.quit();
process.exit(0);
