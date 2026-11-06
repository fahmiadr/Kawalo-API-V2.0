import { EnvConfig, getEnvBool, getEnvInt, getEnvList } from '@core/config/env.config';
import { afterEach, describe, expect, it } from 'vitest';

describe('EnvConfig Unit Tests', () => {
  const originalEnv = { ...process.env };

  afterEach(() => {
    process.env = { ...originalEnv };
  });

  describe('getEnvInt (Safe Integer Parsing & Anti-NaN)', () => {
    it('harus mengembalikan integer valid jika nilai di environment valid', () => {
      process.env.TEST_PORT = '8080';
      expect(getEnvInt('TEST_PORT', 3000)).toBe(8080);
    });

    it('harus fallback ke defaultValue jika variabel tidak terdefinisi', () => {
      delete process.env.TEST_PORT;
      expect(getEnvInt('TEST_PORT', 3000)).toBe(3000);
    });

    it('harus fallback ke defaultValue jika nilai bukan numerik (mencegah Silent NaN)', () => {
      process.env.TEST_PORT = 'invalid_port_abc';
      const result = getEnvInt('TEST_PORT', 3000);
      expect(result).toBe(3000);
      expect(Number.isNaN(result)).toBe(false);
    });

    it('harus memotong desimal jika nilai berupa float string', () => {
      process.env.TEST_COUNT = '42.95';
      expect(getEnvInt('TEST_COUNT', 10)).toBe(42);
    });
  });

  describe('getEnvBool (Flexible Boolean Parsing)', () => {
    it('harus mengembalikan true untuk berbagai variasi positif (true, 1, yes, TRUE)', () => {
      process.env.TEST_FLAG = 'true';
      expect(getEnvBool('TEST_FLAG')).toBe(true);

      process.env.TEST_FLAG = 'TRUE';
      expect(getEnvBool('TEST_FLAG')).toBe(true);

      process.env.TEST_FLAG = '1';
      expect(getEnvBool('TEST_FLAG')).toBe(true);

      process.env.TEST_FLAG = 'yes';
      expect(getEnvBool('TEST_FLAG')).toBe(true);
    });

    it('harus mengembalikan false untuk variasi negatif atau invalid', () => {
      process.env.TEST_FLAG = 'false';
      expect(getEnvBool('TEST_FLAG')).toBe(false);

      process.env.TEST_FLAG = '0';
      expect(getEnvBool('TEST_FLAG')).toBe(false);

      process.env.TEST_FLAG = 'no';
      expect(getEnvBool('TEST_FLAG')).toBe(false);

      process.env.TEST_FLAG = 'random_string';
      expect(getEnvBool('TEST_FLAG')).toBe(false);
    });

    it('harus menggunakan defaultValue jika variabel tidak terdefinisi', () => {
      delete process.env.TEST_FLAG;
      expect(getEnvBool('TEST_FLAG', true)).toBe(true);
      expect(getEnvBool('TEST_FLAG', false)).toBe(false);
    });
  });

  describe('getEnvList (CSV String Array Parsing)', () => {
    it('harus memisahkan string CSV, menghapus whitespace, dan membuang elemen kosong', () => {
      process.env.TEST_LIST = '  10.0.0.0/8 , 192.168.1.0/24 , , 172.16.0.0/12  ';
      const result = getEnvList('TEST_LIST', '');
      expect(result).toEqual(['10.0.0.0/8', '192.168.1.0/24', '172.16.0.0/12']);
    });

    it('harus menggunakan defaultCsv jika variabel tidak terdefinisi', () => {
      delete process.env.TEST_LIST;
      const result = getEnvList('TEST_LIST', '127.0.0.1, ::1');
      expect(result).toEqual(['127.0.0.1', '::1']);
    });
  });

  describe('EnvConfig Singleton & Immutability (GoF Pattern)', () => {
    it('harus dibekukan secara permanen (Object.isFrozen === true)', () => {
      expect(Object.isFrozen(EnvConfig)).toBe(true);
    });

    it('harus melempar error saat mencoba memutasi properti pada strict mode', () => {
      expect(() => {
        // @ts-expect-error Pengujian mutasi runtime
        EnvConfig.PORT = 9999;
      }).toThrow();
    });

    it('harus mengekspos environment flag boolean yang konsisten', () => {
      expect(typeof EnvConfig.isProduction).toBe('boolean');
      expect(typeof EnvConfig.isDevelopment).toBe('boolean');
      expect(typeof EnvConfig.isStaging).toBe('boolean');
      expect(typeof EnvConfig.isTest).toBe('boolean');

      if (EnvConfig.NODE_ENV === 'production') {
        expect(EnvConfig.isProduction).toBe(true);
      } else if (EnvConfig.NODE_ENV === 'development') {
        expect(EnvConfig.isDevelopment).toBe(true);
      }
    });

    it('harus memiliki seluruh konfigurasi numerik yang valid (bebas dari NaN)', () => {
      expect(Number.isFinite(EnvConfig.PORT)).toBe(true);
      expect(Number.isFinite(EnvConfig.JWT_EXPIRES_IN)).toBe(true);
      expect(Number.isFinite(EnvConfig.RATE_LIMIT_MAX)).toBe(true);
      expect(Number.isFinite(EnvConfig.RATE_LIMIT_DURATION)).toBe(true);
    });
  });
});
