import { RsaKeyManager } from '@core/security/rsa-key.manager';
import { TokenService } from '@core/security/token.service';
import { authMiddleware } from '@core/middleware/auth.middleware';
import { Elysia } from 'elysia';
import { beforeAll, describe, expect, it } from 'vitest';

describe('Auth Middleware & Token Sanitization Unit Tests', () => {
  beforeAll(async () => {
    await RsaKeyManager.init();
  });

  it('verifyAccessToken membersihkan nilai currentUser dan mengubah role menjadi uppercase', async () => {
    const { token } = await TokenService.signAccessToken({
      sub: '  USR-001  ',
      email: '  BUDI@kawalo.id  ',
      name: '  Budi Santoso  ',
      role: '  Admin  ',
      group_uid: '  GRP-001  ',
      session_ver: 1,
    });

    const verified = await TokenService.verifyAccessToken(token);

    expect(verified).not.toBeNull();
    expect(verified?.sub).toBe('USR-001');
    expect(verified?.email).toBe('budi@kawalo.id');
    expect(verified?.name).toBe('Budi Santoso');
    expect(verified?.role).toBe('ADMIN'); // Selalu uppercase & bersih
    expect(verified?.group_uid).toBe('GRP-001');
  });

  it('verifyAccessToken mengembalikan undefined jika sub atau email hilang/kosong', async () => {
    const { token } = await TokenService.signAccessToken({
      sub: '   ',
      email: '   ',
      name: 'User',
      session_ver: 1,
    });

    const verified = await TokenService.verifyAccessToken(token);
    expect(verified).not.toBeNull();
    expect(verified?.sub).toBeUndefined();
    expect(verified?.email).toBeUndefined();
  });

  it('requireRole mengizinkan akses jika role cocok tanpa memperhatikan case pada rute', async () => {
    const { token } = await TokenService.signAccessToken({
      sub: 'USR-002',
      email: 'admin@kawalo.id',
      name: 'Admin Test',
      role: 'admin', // Akan di-uppercase saat verify
      session_ver: 1,
    });

    const app = new Elysia().use(authMiddleware).get('/admin-only', () => ({ ok: true }), {
      requireRole: ['admin', 'superadmin'], // lowercase di rute
    });

    const res = await app.handle(
      new Request('http://localhost/admin-only', {
        headers: { Authorization: `Bearer ${token}` },
      }),
    );

    expect(res.status).toBe(200);
    const body = (await res.json()) as { ok: boolean };
    expect(body.ok).toBe(true);
  });

  it('requireRole menolak akses (403 Forbidden) jika role pengguna tidak cocok', async () => {
    const { token } = await TokenService.signAccessToken({
      sub: 'USR-003',
      email: 'user@kawalo.id',
      name: 'User Regular',
      role: 'member',
      session_ver: 1,
    });

    const app = new Elysia().use(authMiddleware).get('/admin-only', () => ({ ok: true }), {
      requireRole: ['admin'],
    });

    const res = await app.handle(
      new Request('http://localhost/admin-only', {
        headers: { Authorization: `Bearer ${token}` },
      }),
    );

    expect(res.status).toBe(403);
  });

  it('requireAuth menolak akses (401 Unauthorized) jika tidak menyertakan Authorization header', async () => {
    const app = new Elysia().use(authMiddleware).get('/protected', () => ({ ok: true }), {
      requireAuth: true,
    });

    const res = await app.handle(new Request('http://localhost/protected'));

    expect(res.status).toBe(401);
    const body = (await res.json()) as { success: boolean; error_code: string };
    expect(body.success).toBe(false);
    expect(body.error_code).toBe('AUTH_UNAUTHORIZED');
  });

  it('requireAuth mengizinkan akses (200 OK) jika token dan sesi valid disertakan', async () => {
    const { token } = await TokenService.signAccessToken({
      sub: 'USR-004',
      email: 'member@kawalo.id',
      name: 'Member Test',
      role: 'USER',
      group_uid: 'GRP-001',
      session_ver: 1,
    });

    const app = new Elysia().use(authMiddleware).get('/protected', () => ({ ok: true }), {
      requireAuth: true,
    });

    const res = await app.handle(
      new Request('http://localhost/protected', {
        headers: { Authorization: `Bearer ${token}` },
      }),
    );

    expect(res.status).toBe(200);
    const body = (await res.json()) as { ok: boolean };
    expect(body.ok).toBe(true);
  });

  it('requireAuth mengizinkan akses jika token valid tanpa group_uid untuk alur non-scoped', async () => {
    const { token } = await TokenService.signAccessToken({
      sub: 'USR-005',
      email: 'member5@kawalo.id',
      name: 'Member 5',
      session_ver: 1,
    });

    const app = new Elysia().use(authMiddleware).get('/protected', () => ({ ok: true }), {
      requireAuth: true,
    });

    const res = await app.handle(
      new Request('http://localhost/protected', {
        headers: { Authorization: `Bearer ${token}` },
      }),
    );

    expect(res.status).toBe(200);
    const body = (await res.json()) as { ok: boolean };
    expect(body.ok).toBe(true);
  });

  it('requireAuth menolak (401 Unauthorized) dengan AUTH_SESSION_STALE jika versi sesi token usang', async () => {
    const { token } = await TokenService.signAccessToken({
      sub: 'USR-006',
      email: 'stale@kawalo.id',
      name: 'Stale User',
      role: 'USER',
      session_ver: 1, // Token membawa versi 1
    });

    // Simulasi: Admin menaikkan versi sesi user di Redis menjadi 2
    const { redis } = await import('@core/database/redis.service');
    const { SessionVersionService } = await import('@core/security/session-version.service');
    await redis.set(SessionVersionService.getKey('USR-006'), '2');

    const app = new Elysia().use(authMiddleware).get('/protected-stale', () => ({ ok: true }), {
      requireAuth: true,
    });

    const res = await app.handle(
      new Request('http://localhost/protected-stale', {
        headers: { Authorization: `Bearer ${token}` },
      }),
    );

    expect(res.status).toBe(401);
    const body = (await res.json()) as { success: boolean; error_code: string; message: string };
    expect(body.success).toBe(false);
    expect(body.error_code).toBe('AUTH_SESSION_STALE');
    expect(body.message).toContain('Sesi telah diperbarui');
  });

  it('verifyAccessToken menolak (mengembalikan null) jika token tidak memiliki klaim session_ver (Strict / Zero Backward-Compatibility)', async () => {
    // Buat JWT manual tanpa klaim session_ver (simulasi token legacy atau malformed)
    const jose = await import('jose');
    const { RsaKeyManager } = await import('@core/security/rsa-key.manager');
    const { AppConstants } = await import('@core/constants/app.constants');

    const privateKey = RsaKeyManager.getPrivateKey();
    const legacyToken = await new jose.SignJWT({
      email: 'legacy@kawalo.id',
      name: 'Legacy User',
      role: 'USER',
      // sengaja tanpa session_ver
    })
      .setProtectedHeader({
        alg: 'RS256',
        typ: 'JWT',
        kid: RsaKeyManager.getActiveKeyId(),
      })
      .setSubject('USR-LEGACY')
      .setIssuer(AppConstants.JWT_ISSUER)
      .setAudience(AppConstants.JWT_AUDIENCE)
      .setIssuedAt()
      .setExpirationTime('15m')
      .sign(privateKey);

    const verified = await TokenService.verifyAccessToken(legacyToken);
    expect(verified).toBeNull();
  });

  it('signAccessToken melemparkan error jika session_ver bernilai 0, negatif, atau tidak bertipe number (Fail-Fast)', async () => {
    // @ts-expect-error Pengujian runtime jika developer lalai menyertakan session_ver
    await expect(
      TokenService.signAccessToken({ sub: 'USR-001', email: 'test@kawalo.id' }),
    ).rejects.toThrow('session_ver wajib berupa bilangan bulat positif');

    await expect(
      TokenService.signAccessToken({ sub: 'USR-001', email: 'test@kawalo.id', session_ver: 0 }),
    ).rejects.toThrow('session_ver wajib berupa bilangan bulat positif');

    await expect(
      TokenService.signAccessToken({ sub: 'USR-001', email: 'test@kawalo.id', session_ver: -1 }),
    ).rejects.toThrow('session_ver wajib berupa bilangan bulat positif');
  });
});
