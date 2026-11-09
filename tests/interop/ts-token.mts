// Verifies a token minted by Rust with the TypeScript TokenService and mints one for Rust to
// verify. Keys come from KEY_DIR (the Rust test's throwaway directory), never from certs/.
import { createPrivateKey, createPublicKey } from 'node:crypto';
import { readFileSync } from 'node:fs';
import { RsaKeyManager } from '@core/security/rsa-key.manager';
import { TokenService } from '@core/security/token.service';

const dir = process.env.KEY_DIR as string;
const privateKey = createPrivateKey(readFileSync(`${dir}/private.key`, 'utf8'));
const publicKey = createPublicKey(readFileSync(`${dir}/public.key`, 'utf8'));
const kid = readFileSync(`${dir}/active_kid.txt`, 'utf8').trim();

RsaKeyManager.getPrivateKey = () => privateKey;
RsaKeyManager.getPublicKey = () => publicKey;
RsaKeyManager.getActiveKeyId = () => kid;

const verified = await TokenService.verifyAccessToken(process.env.RUST_TOKEN as string);
const { token } = await TokenService.signAccessToken({
  sub: 'ts-user',
  email: 'ts@x.id',
  name: 'TS User',
  role: 'guard',
  group_uid: 'g9',
  allow_admin: false,
  allow_patrol: true,
  session_ver: 7,
});
console.log(JSON.stringify({ verified, token }));
