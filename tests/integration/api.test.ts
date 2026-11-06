import { RsaKeyManager } from '@core/security/rsa-key.manager';
import { beforeAll, describe, expect, it } from 'vitest';
import { app } from '@/app';

describe('API Integration Tests (Elysia app.handle)', () => {
  beforeAll(async () => {
    await RsaKeyManager.init();
  });

  it('GET /health harus mengembalikan status ok', async () => {
    const response = await app.handle(new Request('http://localhost:3000/health'));
    expect(response.status).toBe(200);

    const body = (await response.json()) as { success: boolean; data: { status: string } };
    expect(body.success).toBe(true);
    expect(body.data.status).toBe('ok');
  });

  it('GET /health/ready harus memverifikasi status koneksi MariaDB & Redis', async () => {
    const response = await app.handle(new Request('http://localhost:3000/health/ready'));
    const body = (await response.json()) as {
      success: boolean;
      data: {
        status: string;
        database: { status: string };
        redis: { status: string };
      };
    };

    expect(response.status === 200 || response.status === 503).toBe(true);
    expect(body.data).toBeDefined();
    expect(body.data.database).toBeDefined();
    expect(body.data.redis).toBeDefined();
  });

  it('GET /.well-known/jwks.json harus mengembalikan public key RS256', async () => {
    const response = await app.handle(new Request('http://localhost:3000/.well-known/jwks.json'));
    expect(response.status).toBe(200);

    const body = (await response.json()) as {
      keys: Array<{ kty: string; alg: string; [key: string]: unknown }>;
    };
    const firstKey = body.keys[0];
    expect(firstKey).toBeDefined();
    expect(firstKey?.kty).toBe('RSA');
    expect(firstKey?.alg).toBe('RS256');
  });

  it('POST /api/v1/auth/login harus memvalidasi body request', async () => {
    const response = await app.handle(
      new Request('http://localhost:3000/api/v1/auth/login', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: JSON.stringify({ email: 'not-an-email' }),
      }),
    );

    expect(response.status).toBe(422);
    const body = (await response.json()) as {
      success: boolean;
      error_code: string;
      details?: unknown;
    };
    expect(body.success).toBe(false);
    expect(body.error_code).toBe('VALIDATION_ERROR');
  });

  it('POST /api/v1/auth/login dengan JSON malformed harus mengembalikan status 400 dan error_code BAD_REQUEST', async () => {
    const response = await app.handle(
      new Request('http://localhost:3000/api/v1/auth/login', {
        method: 'POST',
        headers: { 'Content-Type': 'application/json' },
        body: '{ malformed json: true,',
      }),
    );

    expect(response.status).toBe(400);
    const body = (await response.json()) as {
      success: boolean;
      message: string;
      error_code: string;
    };
    expect(body.success).toBe(false);
    expect(body.error_code).toBe('BAD_REQUEST');
    expect(body.message).toBe('Format data JSON tidak valid');
  });

  it('Response harus menyertakan header x-request-id dan mempertahankan x-request-id yang dikirim client', async () => {
    const customReqId = 'test-client-req-999';
    const response = await app.handle(
      new Request('http://localhost:3000/.well-known/jwks.json', {
        headers: { 'x-request-id': customReqId },
      }),
    );

    expect(response.status).toBe(200);
    expect(response.headers.get('x-request-id')).toBe(customReqId);
  });

  it('Request ke endpoint tidak terdaftar (404) harus menghasilkan format error standar dan menyertakan x-request-id', async () => {
    const response = await app.handle(new Request('http://localhost:3000/non-existent-endpoint'));
    expect(response.status).toBe(404);
    expect(response.headers.get('x-request-id')).toBeDefined();

    const body = (await response.json()) as { success: boolean; error_code: string };
    expect(body.success).toBe(false);
    expect(body.error_code).toBe('RESOURCE_NOT_FOUND');
  });
});
