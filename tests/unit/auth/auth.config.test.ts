import { AuthConfig } from '@features/auth/config/auth.config';
import { describe, expect, it } from 'vitest';

describe('AuthConfig Unit Tests (Feature-First Configuration)', () => {
  it('harus dibekukan secara permanen (Object.isFrozen === true)', () => {
    expect(Object.isFrozen(AuthConfig)).toBe(true);
  });

  it('harus memiliki nilai numerik yang valid (bebas dari NaN)', () => {
    expect(Number.isFinite(AuthConfig.REFRESH_TOKEN_EXPIRES_IN)).toBe(true);
    expect(Number.isFinite(AuthConfig.LOGIN_RATE_LIMIT_IP_MAX)).toBe(true);
    expect(Number.isFinite(AuthConfig.LOGIN_RATE_LIMIT_DURATION)).toBe(true);
    expect(Number.isFinite(AuthConfig.LOGIN_MAX_FAILED_ATTEMPTS)).toBe(true);
    expect(Number.isFinite(AuthConfig.LOGIN_LOCKOUT_DURATION)).toBe(true);
  });

  it('harus memiliki nilai default yang sesuai dengan standar keamanan sesi', () => {
    expect(AuthConfig.LOGIN_RATE_LIMIT_IP_MAX).toBeGreaterThan(0);
    expect(AuthConfig.LOGIN_RATE_LIMIT_DURATION).toBeGreaterThan(0);
    expect(AuthConfig.LOGIN_MAX_FAILED_ATTEMPTS).toBeGreaterThan(0);
    expect(AuthConfig.LOGIN_LOCKOUT_DURATION).toBeGreaterThan(0);
    expect(AuthConfig.REFRESH_TOKEN_EXPIRES_IN).toBeGreaterThan(0);
  });

  it('calculateRefreshTokenExpiry harus menghasilkan Date yang valid di masa depan', () => {
    const fixedNow = 1700000000000;
    const expiry = AuthConfig.calculateRefreshTokenExpiry(fixedNow);

    expect(expiry).toBeInstanceOf(Date);
    expect(expiry.getTime()).toBe(fixedNow + AuthConfig.REFRESH_TOKEN_EXPIRES_IN * 1000);
  });

  it('harus melempar error saat mencoba memutasi properti pada strict mode', () => {
    expect(() => {
      // @ts-expect-error Mutasi runtime
      AuthConfig.LOGIN_RATE_LIMIT_IP_MAX = 999;
    }).toThrow();
  });
});
