import { RsaKeyManager } from '@core/security/rsa-key.manager';
import { TokenService } from '@core/security/token.service';
import fs from 'fs';
import path from 'path';
import { afterAll, beforeAll, describe, expect, it } from 'vitest';

describe('RsaKeyManager & Key Rotation Unit Tests', () => {
  let initialTestKid: string;

  beforeAll(async () => {
    await RsaKeyManager.init();
    initialTestKid = RsaKeyManager.getActiveKeyId();
  });

  afterAll(async () => {
    // Restore initial active key state so test environment stays clean
    const certsDir = path.resolve(process.cwd(), 'certs');
    const archiveDir = path.join(certsDir, 'archived');
    const privateKeyPath = path.join(certsDir, 'private.key');
    const publicKeyPath = path.join(certsDir, 'public.key');
    const activeKidPath = path.join(certsDir, 'active_kid.txt');

    const restoreKid = initialTestKid;
    if (restoreKid && fs.existsSync(path.join(archiveDir, `${restoreKid}.key`))) {
      fs.copyFileSync(path.join(archiveDir, `${restoreKid}.key`), privateKeyPath);
      fs.copyFileSync(path.join(archiveDir, `${restoreKid}.pub.pem`), publicKeyPath);
      fs.writeFileSync(activeKidPath, restoreKid, 'utf8');

      // Clean up test rotation files
      const files = fs.readdirSync(archiveDir);
      for (const f of files) {
        if (f.startsWith('kawalo-test-rotation-')) {
          try {
            fs.unlinkSync(path.join(archiveDir, f));
          } catch {}
        }
      }

      await RsaKeyManager.init();
    }
  });

  it('initializes active key and exposes valid JWKS', () => {
    const activeKid = RsaKeyManager.getActiveKeyId();
    expect(activeKid).toBeDefined();
    expect(activeKid.length).toBeGreaterThan(0);

    const jwks = RsaKeyManager.getJwks();
    expect(jwks).toBeDefined();
    expect(Array.isArray(jwks.keys)).toBe(true);
    expect(jwks.keys.length).toBeGreaterThan(0);

    const activeKeyJwk = jwks.keys.find((k) => k.kid === activeKid);
    expect(activeKeyJwk).toBeDefined();
    expect(activeKeyJwk?.kty).toBe('RSA');
    expect(activeKeyJwk?.alg).toBe('RS256');
    expect(activeKeyJwk?.use).toBe('sig');
    expect(activeKeyJwk?.n).toBeDefined();
    expect(activeKeyJwk?.e).toBeDefined();
  });

  it('signs access token with active kid in header and verifies it', async () => {
    const payload = {
      sub: 'user-audit-123',
      email: 'audit@kawalo.co.id',
      name: 'Audit User',
      role: 'ADMIN',
      group_uid: 'GRP-001',
      allow_admin: true,
      allow_patrol: true,
      allow_mon_patrol: false,
      session_ver: 1,
    };

    const { token } = await TokenService.signAccessToken(payload);
    expect(token).toBeDefined();

    const verified = await TokenService.verifyAccessToken(token);
    expect(verified).not.toBeNull();
    expect(verified?.sub).toBe('user-audit-123');
    expect(verified?.email).toBe('audit@kawalo.co.id');
    expect(verified?.name).toBe('Audit User');
    expect(verified?.role).toBe('ADMIN');
    expect(verified?.group_uid).toBe('GRP-001');
  });

  it('gracefully rotates key and verifies tokens signed before and after rotation', async () => {
    const initialKid = RsaKeyManager.getActiveKeyId();

    // 1. Sign token with initial active key
    const { token: tokenOld } = await TokenService.signAccessToken({
      sub: 'user-old-key',
      email: 'old@kawalo.co.id',
      name: 'Old User',
      role: 'GUARD',
      group_uid: 'GRP-001',
      allow_admin: false,
      allow_patrol: true,
      allow_mon_patrol: false,
      session_ver: 1,
    });

    // 2. Perform rotation
    const rotationKid = `kawalo-test-rotation-${Date.now()}`;
    const { activeKid, retiredKid } = await RsaKeyManager.rotateKey(rotationKid);

    expect(activeKid).toBe(rotationKid);
    expect(retiredKid).toBe(initialKid);
    expect(RsaKeyManager.getActiveKeyId()).toBe(rotationKid);

    // 3. JWKS should now include both current and retired keys
    const jwksAfterRotation = RsaKeyManager.getJwks();
    const kids = jwksAfterRotation.keys.map((k) => k.kid);
    expect(kids).toContain(rotationKid);
    expect(kids).toContain(initialKid);

    // 4. Sign token with new active key
    const { token: tokenNew } = await TokenService.signAccessToken({
      sub: 'user-new-key',
      email: 'new@kawalo.co.id',
      name: 'New User',
      role: 'MANAGER',
      group_uid: 'GRP-001',
      allow_admin: true,
      allow_patrol: false,
      allow_mon_patrol: false,
      session_ver: 1,
    });

    // 5. Verify both tokens (old and new) successfully
    const verifiedOld = await TokenService.verifyAccessToken(tokenOld);
    expect(verifiedOld).not.toBeNull();
    expect(verifiedOld?.sub).toBe('user-old-key');
    expect(verifiedOld?.role).toBe('GUARD');

    const verifiedNew = await TokenService.verifyAccessToken(tokenNew);
    expect(verifiedNew).not.toBeNull();
    expect(verifiedNew?.sub).toBe('user-new-key');
    expect(verifiedNew?.role).toBe('MANAGER');
  }, 15000);

  it('generates consistent key id format matching kawalo-key-<timestamp>', () => {
    const generatedKid = RsaKeyManager.generateKeyId();
    expect(generatedKid).toMatch(/^kawalo-key-\d+$/);
  });

  it('prunes archived keys without errors and retains valid keys', () => {
    const pruneResult = RsaKeyManager.pruneArchivedKeys(30);
    expect(pruneResult).toBeDefined();
    expect(Array.isArray(pruneResult.prunedKids)).toBe(true);
    expect(Array.isArray(pruneResult.retainedKids)).toBe(true);
  });

  it('filters archived keys from JWKS if older than jwksMaxAgeDays (7 days)', () => {
    const activeKid = RsaKeyManager.getActiveKeyId();
    expect(RsaKeyManager.getJwks().keys.some((k) => k.kid === activeKid)).toBe(true);

    const originalMaxAge = RsaKeyManager.getJwksMaxAgeDays();
    expect(originalMaxAge).toBe(7);

    try {
      // Temporarily set jwksMaxAgeDays to 0 (all archived keys excluded from JWKS)
      RsaKeyManager.setJwksMaxAgeDays(0);
      const jwks = RsaKeyManager.getJwks();

      // Only the active key should be exposed in JWKS
      expect(jwks.keys.length).toBe(1);
      expect(jwks.keys[0]?.kid).toBe(activeKid);
    } finally {
      RsaKeyManager.setJwksMaxAgeDays(originalMaxAge);
    }
  });
});
