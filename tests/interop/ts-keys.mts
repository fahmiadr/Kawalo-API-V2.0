// Generates a key pair exactly the way rsa-key.manager.ts does (spki/pkcs8 PEM) and signs a token
// with the TypeScript TokenService, so Rust can prove it loads Node-written key files.
import { generateKeyPairSync, createPrivateKey, createPublicKey } from 'node:crypto';
import { mkdirSync, writeFileSync } from 'node:fs';
import { RsaKeyManager } from '@core/security/rsa-key.manager';
import { TokenService } from '@core/security/token.service';

const dir = process.env.KEY_DIR as string;
const kid = 'kawalo-key-node';
const { privateKey, publicKey } = generateKeyPairSync('rsa', {
  modulusLength: 2048,
  publicKeyEncoding: { type: 'spki', format: 'pem' },
  privateKeyEncoding: { type: 'pkcs8', format: 'pem' },
});
mkdirSync(dir, { recursive: true });
writeFileSync(`${dir}/private.key`, privateKey, 'utf8');
writeFileSync(`${dir}/public.key`, publicKey, 'utf8');
writeFileSync(`${dir}/active_kid.txt`, kid, 'utf8');

const priv = createPrivateKey(privateKey);
const pub = createPublicKey(publicKey);
RsaKeyManager.getPrivateKey = () => priv;
RsaKeyManager.getPublicKey = () => pub;
RsaKeyManager.getActiveKeyId = () => kid;

const { token } = await TokenService.signAccessToken({
  sub: 'node-user',
  email: 'node@x.id',
  role: 'admin',
  group_uid: 'g3',
  session_ver: 2,
});
console.log(JSON.stringify({ token }));
