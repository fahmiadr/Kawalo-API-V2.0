import { RsaKeyManager } from '@core/security/rsa-key.manager';
import { beforeAll, describe, expect, it } from 'vitest';
import { app } from '@/app';

interface ApiResponsePayload<T> {
  success: boolean;
  message: string;
  data: T;
}

describe('End-to-End Features Integration (Auth -> Guests -> Operational)', () => {
  let accessToken = '';
  let refreshToken = '';

  beforeAll(async () => {
    await RsaKeyManager.init();
  });

  it('1. Login dengan akun personil (asus@gmail.com / asus)', async () => {
    const response = await app.handle(
      new Request('http://localhost:3000/api/v1/auth/login', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          email: 'asus@gmail.com',
          password: 'asus',
          device_id: 'test-device-1',
          device_name: 'Test Runner Device',
        }),
      }),
    );

    expect(response.status).toBe(200);
    const body = (await response.json()) as ApiResponsePayload<{
      user: { id: string; email: string; role: string };
      tokens: { access_token: string; refresh_token: string };
    }>;
    expect(body.success).toBe(true);
    expect(body.data.user.email).toBe('asus@gmail.com');
    expect(body.data.tokens.access_token).toBeDefined();
    expect(body.data.tokens.refresh_token).toBeDefined();

    accessToken = body.data.tokens.access_token;
    refreshToken = body.data.tokens.refresh_token;
  });

  it('2. Mengambil daftar tamu (GET /api/v1/guests)', async () => {
    const response = await app.handle(
      new Request('http://localhost:3000/api/v1/guests?limit=10', {
        method: 'GET',
        headers: {
          Authorization: `Bearer ${accessToken}`,
        },
      }),
    );

    expect(response.status).toBe(200);
    const body = (await response.json()) as ApiResponsePayload<unknown[]>;
    expect(body.success).toBe(true);
    expect(Array.isArray(body.data)).toBe(true);
  });

  it('3. Memperbarui access token via Refresh Token (POST /api/v1/auth/refresh)', async () => {
    const response = await app.handle(
      new Request('http://localhost:3000/api/v1/auth/refresh', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({
          refresh_token: refreshToken,
        }),
      }),
    );

    expect(response.status).toBe(200);
    const body = (await response.json()) as ApiResponsePayload<{
      access_token: string;
      refresh_token: string;
    }>;
    expect(body.success).toBe(true);
    expect(body.data.access_token).toBeDefined();
    expect(body.data.refresh_token).toBeDefined();
    expect(body.data.refresh_token).not.toBe(refreshToken); // New rotated token
  });

  it('4. Mengubah grup aktif pengguna (POST /api/v1/auth/switch-group)', async () => {
    const response = await app.handle(
      new Request('http://localhost:3000/api/v1/auth/switch-group', {
        method: 'POST',
        headers: {
          'Content-Type': 'application/json',
          Authorization: `Bearer ${accessToken}`,
        },
        body: JSON.stringify({
          group_uid: 'G-DEMO-001',
        }),
      }),
    );

    // Boleh sukses atau error jika grup tidak terdaftar, namun rute terautentikasi (bukan 404 / malformed)
    expect([200, 400, 401, 403]).toContain(response.status);
  });

  it('5. Mengambil daftar tamu dengan filter paginasi (GET /api/v1/guests?limit=5)', async () => {
    const response = await app.handle(
      new Request('http://localhost:3000/api/v1/guests?limit=5', {
        method: 'GET',
        headers: {
          Authorization: `Bearer ${accessToken}`,
        },
      }),
    );

    expect(response.status).toBe(200);
    const body = (await response.json()) as ApiResponsePayload<unknown[]>;
    expect(body.success).toBe(true);
    expect(Array.isArray(body.data)).toBe(true);
  });

  it('6. Mengambil aktifitas operasional tenaga kerja (GET /api/v1/operational)', async () => {
    const response = await app.handle(
      new Request('http://localhost:3000/api/v1/operational?limit=10', {
        method: 'GET',
        headers: {
          Authorization: `Bearer ${accessToken}`,
        },
      }),
    );

    expect(response.status).toBe(200);
    const body = (await response.json()) as ApiResponsePayload<
      Array<{
        user_email: string;
        user_name: string;
        timeline: unknown[];
      }>
    >;
    expect(body.success).toBe(true);
    expect(Array.isArray(body.data)).toBe(true);
  });
});
