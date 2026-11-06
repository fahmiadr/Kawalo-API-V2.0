import { getEnvInt } from '@core/config/env.config';

export interface AuthConfigType {
  readonly REFRESH_TOKEN_EXPIRES_IN: number;
  readonly LOGIN_RATE_LIMIT_IP_MAX: number;
  readonly LOGIN_RATE_LIMIT_DURATION: number;
  readonly LOGIN_MAX_FAILED_ATTEMPTS: number;
  readonly LOGIN_LOCKOUT_DURATION: number;
  calculateRefreshTokenExpiry(now?: number): Date;
}

/**
 * Feature-First Configuration untuk Domain Autentikasi & Keamanan Sesi.
 * Mengenkapsulasi batas percobaan login, durasi lockout, dan masa berlaku refresh token.
 */
export const AuthConfig: AuthConfigType = Object.freeze({
  REFRESH_TOKEN_EXPIRES_IN: getEnvInt('REFRESH_TOKEN_EXPIRES_IN', 1209600), // 14 hari dalam detik
  LOGIN_RATE_LIMIT_IP_MAX: getEnvInt('LOGIN_RATE_LIMIT_IP_MAX', 10),
  LOGIN_RATE_LIMIT_DURATION: getEnvInt('LOGIN_RATE_LIMIT_DURATION', 900), // 15 menit dalam detik
  LOGIN_MAX_FAILED_ATTEMPTS: getEnvInt('LOGIN_MAX_FAILED_ATTEMPTS', 5),
  LOGIN_LOCKOUT_DURATION: getEnvInt('LOGIN_LOCKOUT_DURATION', 900), // 15 menit dalam detik

  /**
   * Menghitung tanggal kedaluwarsa sesi Refresh Token berdasarkan kebijakan masa berlaku.
   */
  calculateRefreshTokenExpiry(now = Date.now()): Date {
    return new Date(now + AuthConfig.REFRESH_TOKEN_EXPIRES_IN * 1000);
  },
});
