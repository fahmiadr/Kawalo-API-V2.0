import { RsaKeyManager } from '@core/security/rsa-key.manager';
import { SessionVersionService } from '@core/security/session-version.service';
import { TokenService } from '@core/security/token.service';
import { node } from '@elysiajs/node';
import { pttWsRoute } from '@features/ptt/presentation/ptt.ws';
import { Elysia } from 'elysia';
import { beforeAll, describe, expect, it, vi } from 'vitest';

describe('PTT WebSocket Route (ptt.ws.ts) Handshake & Auth Tests', () => {
  let app: Elysia;
  const groupUid = 'group_102';

  beforeAll(async () => {
    await RsaKeyManager.init();
    app = new Elysia({ adapter: node() }).use(pttWsRoute);
  });

  it('harus menolak handshake dengan status 401 jika access token tidak disertakan', async () => {
    const response = await app.handle(
      new Request(`http://localhost/ws/ptt/${groupUid}`, {
        headers: {
          upgrade: 'websocket',
        },
      }),
    );

    expect(response.status).toBe(401);
    const body = await response.text();
    expect(body).toContain('Unauthorized: Missing access token');
  });

  it('harus menolak handshake dengan status 401 jika access token tidak valid / rusak', async () => {
    const response = await app.handle(
      new Request(`http://localhost/ws/ptt/${groupUid}?token=invalid.token.here`, {
        headers: {
          upgrade: 'websocket',
        },
      }),
    );

    expect(response.status).toBe(401);
    const body = await response.text();
    expect(body).toContain('Unauthorized: Invalid access token signature');
  });

  it('harus menolak handshake dengan status 401 jika session_ver token telah stale / revoked di Redis', async () => {
    const { token } = await TokenService.signAccessToken({
      sub: 'usr_revoked_1',
      email: 'revoked@kawalo.id',
      name: 'Revoked Guard',
      role: 'GUARD',
      group_uid: groupUid,
      session_ver: 1,
    });

    // Simulasikan Redis memiliki session_ver = 2 (token lama stale)
    vi.spyOn(SessionVersionService, 'validateOrRehydrate').mockResolvedValueOnce(false);

    const response = await app.handle(
      new Request(`http://localhost/ws/ptt/${groupUid}?token=${token}`, {
        headers: {
          upgrade: 'websocket',
        },
      }),
    );

    expect(response.status).toBe(401);
    const body = await response.text();
    expect(body).toContain('Unauthorized: Session revoked or expired');
  });

  it('harus menolak handshake dengan status 403 jika pengguna tidak terdaftar di group tersebut (Tenancy Isolation)', async () => {
    const { token } = await TokenService.signAccessToken({
      sub: 'usr_guard_other',
      email: 'other@kawalo.id',
      name: 'Other Guard',
      role: 'GUARD',
      group_uid: 'group_different_999', // Grup berbeda
      session_ver: 1,
    });

    vi.spyOn(SessionVersionService, 'validateOrRehydrate').mockResolvedValueOnce(true);

    const response = await app.handle(
      new Request(`http://localhost/ws/ptt/${groupUid}?token=${token}`, {
        headers: {
          upgrade: 'websocket',
        },
      }),
    );

    expect(response.status).toBe(403);
    const body = await response.text();
    expect(body).toContain('Forbidden: User does not belong to this squad group');
  });

  it('harus mengizinkan handshake jika token valid dan group_uid cocok', async () => {
    const { token } = await TokenService.signAccessToken({
      sub: 'usr_guard_valid',
      email: 'valid@kawalo.id',
      name: 'Valid Guard',
      role: 'GUARD',
      group_uid: groupUid,
      session_ver: 1,
    });

    vi.spyOn(SessionVersionService, 'validateOrRehydrate').mockResolvedValueOnce(true);

    // Di unit testing HTTP handle, Elysia sebelum handle mengembalikan undefined jika lolos guard (siap upgrade websocket)
    const response = await app.handle(
      new Request(`http://localhost/ws/ptt/${groupUid}?token=${token}`, {
        headers: {
          upgrade: 'websocket',
        },
      }),
    );

    // Jika lolos guard HTTP upgrade, statusnya bukan 401 dan bukan 403
    expect(response.status).not.toBe(401);
    expect(response.status).not.toBe(403);
  });

  it('harus mengizinkan SUPER_ADMIN mengakses channel grup manapun', async () => {
    const { token } = await TokenService.signAccessToken({
      sub: 'usr_super_admin',
      email: 'super@kawalo.id',
      name: 'Super Admin',
      role: 'SUPER_ADMIN',
      group_uid: 'group_headquarters',
      session_ver: 1,
    });

    vi.spyOn(SessionVersionService, 'validateOrRehydrate').mockResolvedValueOnce(true);

    const response = await app.handle(
      new Request(`http://localhost/ws/ptt/${groupUid}?token=${token}`, {
        headers: {
          upgrade: 'websocket',
        },
      }),
    );

    expect(response.status).not.toBe(401);
    expect(response.status).not.toBe(403);
  });

  it('harus menerima token dari Authorization Header (Bearer) maupun query parameter', async () => {
    const { token } = await TokenService.signAccessToken({
      sub: 'usr_guard_header',
      email: 'header@kawalo.id',
      name: 'Header Guard',
      role: 'GUARD',
      group_uid: groupUid,
      session_ver: 1,
    });

    vi.spyOn(SessionVersionService, 'validateOrRehydrate').mockResolvedValueOnce(true);

    const response = await app.handle(
      new Request(`http://localhost/ws/ptt/${groupUid}`, {
        headers: {
          upgrade: 'websocket',
          authorization: `Bearer ${token}`,
        },
      }),
    );

    expect(response.status).not.toBe(401);
    expect(response.status).not.toBe(403);
  });
});
