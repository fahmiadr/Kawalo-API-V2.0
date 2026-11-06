import { AppConstants } from '@core/constants/app.constants';
import { describe, expect, it } from 'vitest';

describe('AppConstants Tests', () => {
  it('harus mengembalikan prefix default /api/v1 saat dipanggil tanpa argumen', () => {
    expect(AppConstants.apiVersion()).toBe('/api/v1');
  });

  it('harus mendukung berbagai format versi (v1, v2, v3, number 2, string 3)', () => {
    expect(AppConstants.apiVersion('v1')).toBe('/api/v1');
    expect(AppConstants.apiVersion('v2')).toBe('/api/v2');
    expect(AppConstants.apiVersion('v3')).toBe('/api/v3');
    expect(AppConstants.apiVersion(2)).toBe('/api/v2');
    expect(AppConstants.apiVersion('3')).toBe('/api/v3');
  });

  it('harus memiliki base path /api dan default version v1', () => {
    expect(AppConstants.API_BASE_PATH).toBe('/api');
    expect(AppConstants.DEFAULT_API_VERSION).toBe('v1');
  });

  it('harus mendefinisikan konstanta aplikasi dan otentikasi dengan benar', () => {
    expect(AppConstants.APP_NAME).toBe('Kawalo Mobile API');
    expect(AppConstants.AUTH_TOKEN_TYPE).toBe('Bearer');
    expect(AppConstants.JWT_ISSUER).toBe('kawalo_auth');
    expect(AppConstants.JWT_AUDIENCE).toBe('kawalo_mobile');
  });

  it('harus mendefinisikan batasan paginasi (DEFAULT: 20, MAX: 100)', () => {
    expect(AppConstants.DEFAULT_PAGE_SIZE).toBe(20);
    expect(AppConstants.MAX_PAGE_SIZE).toBe(100);
    expect(AppConstants.DEFAULT_PAGE_SIZE).toBeLessThanOrEqual(AppConstants.MAX_PAGE_SIZE);
  });
});
