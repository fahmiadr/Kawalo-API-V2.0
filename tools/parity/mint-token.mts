// Mints the access tokens used by the parity scenarios and seeds the matching session_ver keys in
// every Redis instance, so server A and server B see exactly the same sessions.
//   $KW_TS_ROOT/node_modules/.bin/tsx --tsconfig $KW_TS_ROOT/tsconfig.json tools/parity/mint-token.mts --redis redis://h:p,redis://h:p --out tokens.json
// Run from a directory whose ./certs points at the repo certs; key material is never printed.
import { writeFileSync } from 'node:fs';
import { generateKeyPairSync } from 'node:crypto';
import { RsaKeyManager } from '@core/security/rsa-key.manager';
import { TokenService } from '@core/security/token.service';
import { AppConstants } from '@core/constants/app.constants';
import Redis from 'ioredis';
import * as jose from 'jose';

const arg = (name: string): string | undefined => {
  const i = process.argv.indexOf(`--${name}`);
  return i > 0 ? process.argv[i + 1] : undefined;
};
const redisUrls = (arg('redis') ?? '').split(',').filter(Boolean);
const outPath = arg('out') ?? 'tokens.json';

type Claims = {
  sub?: string;
  email?: string;
  name?: string;
  role?: string | null;
  group_uid?: string | null;
  allow_admin?: boolean;
  allow_patrol?: boolean;
  allow_mon_patrol?: boolean;
  session_ver?: number | string;
};

await RsaKeyManager.init();
const kid = RsaKeyManager.getActiveKeyId();
const privateKey = RsaKeyManager.getPrivateKey();
const tokens: Record<string, string> = {};
const sessionVersions = new Map<string, number>();

const ALPHA = 'grp-alpha';
const BETA = 'grp-beta';
const GAMMA = 'grp-gamma';

// The standard path goes through the real TokenService.
async function std(name: string, c: Required<Pick<Claims, 'sub' | 'email' | 'name'>> & Claims) {
  const ver = typeof c.session_ver === 'number' ? c.session_ver : 1;
  const { token } = await TokenService.signAccessToken({
    sub: c.sub,
    email: c.email,
    name: c.name,
    role: c.role ?? null,
    group_uid: c.group_uid ?? null,
    allow_admin: c.allow_admin ?? false,
    allow_patrol: c.allow_patrol ?? false,
    allow_mon_patrol: c.allow_mon_patrol ?? false,
    session_ver: ver,
  });
  tokens[name] = token;
  if (!sessionVersions.has(c.sub)) sessionVersions.set(c.sub, ver);
}

// Hand-built variants for the negative cases. Everything else mirrors TokenService.
async function raw(
  name: string,
  claims: Record<string, unknown>,
  opts: {
    header?: Record<string, unknown>;
    key?: jose.KeyLike | Uint8Array;
    iss?: string | null;
    aud?: string | null;
    exp?: string | number;
    iat?: number;
    alg?: string;
  } = {},
) {
  let b = new jose.SignJWT(claims)
    .setProtectedHeader({ alg: opts.alg ?? 'RS256', typ: 'JWT', kid, ...opts.header })
    .setIssuedAt(opts.iat);
  if (opts.iss !== null) b = b.setIssuer(opts.iss ?? AppConstants.JWT_ISSUER);
  if (opts.aud !== null) b = b.setAudience(opts.aud ?? AppConstants.JWT_AUDIENCE);
  b = b.setExpirationTime(opts.exp ?? '1h');
  tokens[name] = await b.sign(opts.key ?? privateKey);
}

const base = (sub: string, email: string, name: string, role: string, group: string | null) => ({
  sub,
  email,
  name,
  role,
  group_uid: group,
  allow_admin: false,
  allow_patrol: false,
  allow_mon_patrol: false,
  session_ver: 1,
});

// ---- standard users (sub = tkuser.UserId so session keys line up with login)
await std('admin_g1', { ...base('1', 'admin@alpha.test', 'Andi Admin', 'ADMIN', ALPHA), allow_admin: true, allow_patrol: true, allow_mon_patrol: true });
await std('guard1_g1', { ...base('2', 'guard1@alpha.test', 'Budi Penjaga', 'GUARD', ALPHA), allow_patrol: true });
await std('guard2_g1', { ...base('3', 'guard2@alpha.test', 'Citra Penjaga', 'GUARD', ALPHA), allow_patrol: true });
await std('resident_g1', base('4', 'resident@alpha.test', 'Dewi Warga', 'USER', ALPHA));
await std('resident2_g1', base('5', 'resident2@alpha.test', 'eko warga', 'USER', ALPHA));
await std('resident3_g1', base('6', 'resident3@alpha.test', 'Álvaro Ñandú', 'USER', ALPHA));
await std('supervisor_g1', { ...base('12', 'supervisor@alpha.test', 'Lina Supervisor', 'GUARD', ALPHA), allow_admin: true });
await std('admin2_g1', base('13', 'admin2@alpha.test', 'Maman Admin Dua', 'ADMIN', ALPHA));
await std('admin_g2', { ...base('8', 'admin@beta.test', 'Gita Admin', 'ADMIN', BETA), allow_admin: true });
await std('guard_g2', base('9', 'guard@beta.test', 'Hadi Beta', 'GUARD', BETA));
await std('admin_g3', { ...base('20', 'lock1@gamma.test', 'Lock Satu', 'ADMIN', GAMMA), allow_admin: true });
await std('guard_g3', base('21', 'lock2@gamma.test', 'Lock Dua', 'GUARD', GAMMA));
// users without an active group
await std('admin_nogroup', { ...base('10', 'nogroup@test.id', 'Indra Tanpa Grup', 'ADMIN', null) });
await std('user_nogroup', { ...base('10', 'nogroup@test.id', 'Indra Tanpa Grup', '', null), role: null });
// role / claim normalisation quirks
await std('guard_lowercase_role', base('2', 'guard1@alpha.test', 'Budi Penjaga', 'guard', ALPHA));
await std('viewer_role', base('4', 'resident@alpha.test', 'Dewi Warga', 'VIEWER', ALPHA));
await std('guard_upper_email', base('2', 'GUARD1@ALPHA.TEST', '  Budi Penjaga  ', 'GUARD', ALPHA));
await std('guard_na_role', base('2', 'guard1@alpha.test', 'Budi Penjaga', 'N/A', ALPHA));
// guard in the wrong tenant for ptt and a super admin that may cross groups
await std('super_admin', base('1', 'admin@alpha.test', 'Andi Admin', 'SUPER_ADMIN', ALPHA));
// extra identities for ptt/sse so scenarios do not share sockets with the same user id
for (let i = 1; i <= 6; i++) {
  await std(`ptt_guard${i}`, base(String(300 + i), `ptt${i}@alpha.test`, `Ptt Guard ${i}`, 'GUARD', ALPHA));
}
await std('ptt_admin1', base('310', 'pttadmin1@alpha.test', 'Ptt Admin Satu', 'ADMIN', ALPHA));
await std('ptt_admin2', base('311', 'pttadmin2@alpha.test', 'Ptt Admin Dua', 'ADMIN', ALPHA));
await std('ptt_beta_guard', base('320', 'pttbeta@beta.test', 'Ptt Beta', 'GUARD', BETA));
await std('ptt_nogroup', base('321', 'pttnog@test.id', 'Ptt NoGroup', 'GUARD', null));
await std('sse_guard1', base('330', 'sse1@alpha.test', 'Sse Satu', 'GUARD', ALPHA));
await std('sse_guard2', base('331', 'sse2@alpha.test', 'Sse Dua', 'GUARD', ALPHA));
await std('sse_admin', { ...base('332', 'sseadmin@alpha.test', 'Sse Admin', 'ADMIN', ALPHA), allow_admin: true });

// ---- session_ver behaviours
await std('stale_user', { ...base('2000', 'stale@x.test', 'Stale User', 'GUARD', ALPHA), session_ver: 1 });
sessionVersions.set('2000', 5);
await raw('rehydrate_v3', { ...base('2001', 'rehydrate@x.test', 'Rehydrate', 'GUARD', ALPHA), session_ver: 3 });
await raw('rehydrate_v2', { ...base('2001', 'rehydrate@x.test', 'Rehydrate', 'GUARD', ALPHA), session_ver: 2 });
await raw('newer_than_redis', { ...base('2002', 'newer@x.test', 'Newer', 'GUARD', ALPHA), session_ver: 9 });
sessionVersions.set('2002', 2);
await raw('ver_as_string', { ...base('2003', 'verstr@x.test', 'Ver String', 'GUARD', ALPHA), session_ver: '1' });
sessionVersions.set('2003', 1);
await raw('ver_zero', { ...base('2004', 'verzero@x.test', 'Ver Zero', 'GUARD', ALPHA), session_ver: 0 });
await raw('ver_missing', (({ session_ver: _v, ...rest }) => rest)(base('2005', 'vermissing@x.test', 'Ver Missing', 'GUARD', ALPHA)));
await raw('ver_float', { ...base('2006', 'verfloat@x.test', 'Ver Float', 'GUARD', ALPHA), session_ver: 1.9 });
sessionVersions.set('2006', 1);

// ---- broken tokens
await raw('expired', base('2', 'guard1@alpha.test', 'Budi Penjaga', 'GUARD', ALPHA), {
  iat: Math.floor(Date.now() / 1000) - 7200,
  exp: Math.floor(Date.now() / 1000) - 3600,
});
await raw('wrong_issuer', base('2', 'guard1@alpha.test', 'Budi Penjaga', 'GUARD', ALPHA), { iss: 'someone_else' });
await raw('wrong_audience', base('2', 'guard1@alpha.test', 'Budi Penjaga', 'GUARD', ALPHA), { aud: 'other_app' });
await raw('missing_issuer', base('2', 'guard1@alpha.test', 'Budi Penjaga', 'GUARD', ALPHA), { iss: null });
await raw('missing_sub', (({ sub: _s, ...rest }) => rest)(base('2', 'guard1@alpha.test', 'Budi Penjaga', 'GUARD', ALPHA)));
await raw('missing_email', (({ email: _e, ...rest }) => rest)(base('2', 'guard1@alpha.test', 'Budi Penjaga', 'GUARD', ALPHA)));
await raw('unknown_kid', base('2', 'guard1@alpha.test', 'Budi Penjaga', 'GUARD', ALPHA), { header: { kid: 'kawalo-key-0' } });
const other = generateKeyPairSync('rsa', { modulusLength: 2048 });
await raw('bad_signature', base('2', 'guard1@alpha.test', 'Budi Penjaga', 'GUARD', ALPHA), { key: other.privateKey });
await raw('hs256_token', base('2', 'guard1@alpha.test', 'Budi Penjaga', 'GUARD', ALPHA), {
  alg: 'HS256',
  key: new TextEncoder().encode('0123456789abcdef0123456789abcdef'),
});
const goodParts = tokens.guard1_g1?.split('.') ?? [];
const b64 = (o: unknown) => Buffer.from(JSON.stringify(o)).toString('base64url');
tokens.alg_none = `${b64({ alg: 'none', typ: 'JWT' })}.${goodParts[1] ?? ''}.`;
tokens.tampered_payload = `${goodParts[0]}.${b64({ ...base('1', 'admin@alpha.test', 'Andi Admin', 'ADMIN', ALPHA), session_ver: 1, iss: AppConstants.JWT_ISSUER, aud: AppConstants.JWT_AUDIENCE, exp: Math.floor(Date.now() / 1000) + 3600 })}.${goodParts[2]}`;
tokens.garbage = 'not-a-jwt';
tokens.two_parts = `${goodParts[0]}.${goodParts[1]}`;

if (redisUrls.length > 0) {
  for (const url of redisUrls) {
    const r = new Redis(url, { maxRetriesPerRequest: 2 });
    for (const [sub, ver] of sessionVersions) await r.set(`auth:user:${sub}:session_ver`, String(ver), 'EX', 1209600);
    await r.quit();
  }
}

writeFileSync(outPath, JSON.stringify(tokens, null, 2));
console.log(`minted ${Object.keys(tokens).length} tokens -> ${outPath}; session_ver set for ${sessionVersions.size} users on ${redisUrls.length} redis`);
process.exit(0);
