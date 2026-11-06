import dotenv from 'dotenv';
import path from 'path';

// 1. Eksekusi pemuatan Dotenv secara terarah
const rawNodeEnv = process.env.NODE_ENV?.trim().toLowerCase() || 'development';
const envFile =
  rawNodeEnv === 'production'
    ? '.env.prod'
    : rawNodeEnv === 'staging'
      ? '.env.staging'
      : '.env.dev';

dotenv.config({ path: path.resolve(process.cwd(), envFile) });
dotenv.config(); // Fallback ke default .env lokal jika ada

// 2. Helper Parser Aman (Mencegah Silent NaN & Rigid Boolean Anti-Pattern)
export function getEnvInt(key: string, defaultValue: number): number {
  const raw = process.env[key];
  if (!raw) return defaultValue;
  const parsed = Number.parseInt(raw, 10);
  return Number.isFinite(parsed) ? parsed : defaultValue;
}

export function getEnvBool(key: string, defaultValue = false): boolean {
  const raw = process.env[key]?.trim().toLowerCase();
  if (!raw) return defaultValue;
  return raw === 'true' || raw === '1' || raw === 'yes';
}

export function getEnvList(key: string, defaultCsv: string): string[] {
  const raw = process.env[key] || defaultCsv;
  return raw
    .split(',')
    .map((item) => item.trim())
    .filter(Boolean);
}

// 3. Kontrak Interface Konfigurasi Shared Kernel (Platform-wide)
export interface EnvConfigType {
  readonly NODE_ENV: string;
  readonly isProduction: boolean;
  readonly isDevelopment: boolean;
  readonly isStaging: boolean;
  readonly isTest: boolean;

  readonly PORT: number;
  readonly LOG_LEVEL: string;
  readonly DATABASE_URL: string;
  readonly DATABASE_USE_TEXT_PROTOCOL: boolean;
  readonly REDIS_URL: string;
  readonly JWT_EXPIRES_IN: number;
  readonly CORS_ORIGIN: string;
  readonly RATE_LIMIT_MAX: number;
  readonly RATE_LIMIT_DURATION: number;
  readonly TRUSTED_PROXY_CIDRS: string[];
}

// 4. Inisialisasi Immutable Singleton (GoF Pattern)
export const EnvConfig: EnvConfigType = Object.freeze({
  NODE_ENV: rawNodeEnv,
  isProduction: rawNodeEnv === 'production',
  isDevelopment: rawNodeEnv === 'development',
  isStaging: rawNodeEnv === 'staging',
  isTest: rawNodeEnv === 'test',

  PORT: getEnvInt('PORT', 3000),
  LOG_LEVEL: process.env.LOG_LEVEL || (rawNodeEnv === 'production' ? 'info' : 'debug'),
  DATABASE_URL: process.env.DATABASE_URL || 'mysql://root:@localhost:3306/kawalo',
  DATABASE_USE_TEXT_PROTOCOL: getEnvBool('DATABASE_USE_TEXT_PROTOCOL', false),
  REDIS_URL: process.env.REDIS_URL || 'redis://localhost:6379',
  JWT_EXPIRES_IN: getEnvInt('JWT_EXPIRES_IN', 900),
  CORS_ORIGIN: process.env.CORS_ORIGIN || '*',
  RATE_LIMIT_MAX: getEnvInt('RATE_LIMIT_MAX', 100),
  RATE_LIMIT_DURATION: getEnvInt('RATE_LIMIT_DURATION', 60000),
  TRUSTED_PROXY_CIDRS: getEnvList(
    'TRUSTED_PROXY_CIDRS',
    '127.0.0.1/32,::1/128,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16',
  ),
});
