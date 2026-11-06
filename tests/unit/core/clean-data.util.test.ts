import {
  cleanBoolean,
  cleanCoordinate,
  cleanDate,
  cleanInteger,
  cleanNumber,
  cleanQueryBoolean,
  cleanQueryCoordinate,
  cleanQueryInteger,
  cleanString,
  cleanUpdateBoolean,
  cleanUpdateCoordinate,
  cleanUpdateDate,
  cleanUpdateInteger,
  cleanUpdateNumber,
  cleanUpdateString,
} from '@core/utils/clean-data.util';
import { describe, expect, it } from 'vitest';

describe('clean-data.util Unit Tests', () => {
  describe('cleanString with automatic email detection & lowercasing', () => {
    it('mengembalikan null untuk input null, undefined, spasi kosong, N/A, dan null literal', () => {
      expect(cleanString(null)).toBeNull();
      expect(cleanString(undefined)).toBeNull();
      expect(cleanString('')).toBeNull();
      expect(cleanString('   ')).toBeNull();
      expect(cleanString('N/A')).toBeNull();
      expect(cleanString('  N/A  ')).toBeNull();
      expect(cleanString('null')).toBeNull();
      expect(cleanString('  null  ')).toBeNull();
    });

    it('otomatis mendeteksi email valid dan mengubahnya menjadi lowercase', () => {
      expect(cleanString('Asus@Gmail.Com')).toBe('asus@gmail.com');
      expect(cleanString('  Satpam1@Kawalo.Id  ')).toBe('satpam1@kawalo.id');
      expect(cleanString('USER.NAME+TAG@COMPANY.CO.ID')).toBe('user.name+tag@company.co.id');
      expect(cleanString('admin_patrol@sub.domain.org')).toBe('admin_patrol@sub.domain.org');
    });

    it('mempertahankan casing asli untuk teks non-email', () => {
      expect(cleanString('Budi Santoso')).toBe('Budi Santoso');
      expect(cleanString('GRP-SECURITY-01')).toBe('GRP-SECURITY-01');
      expect(cleanString('Shift Pagi (08:00 - 16:00)')).toBe('Shift Pagi (08:00 - 16:00)');
      expect(cleanString('Jl. Jend. Sudirman Kav. 52-53')).toBe('Jl. Jend. Sudirman Kav. 52-53');
      expect(cleanString('NOT_AN_EMAIL@')).toBe('NOT_AN_EMAIL@');
      expect(cleanString('@invalid.com')).toBe('@invalid.com');
      expect(cleanString('user@localhost')).toBe('user@localhost'); // Bukan email dengan TLD bertingkat
    });
  });

  describe('cleanDate', () => {
    it('mengembalikan null untuk epoch legacy 1970 atau 0000', () => {
      expect(cleanDate('0000-00-00 00:00:00')).toBeNull();
      expect(cleanDate('1970-01-01 00:00:00')).toBeNull();
      expect(cleanDate('1970-01-01T00:00:00.000Z')).toBeNull();
      expect(cleanDate('N/A')).toBeNull();
      expect(cleanDate(null)).toBeNull();
    });

    it('mengonversi tanggal valid menjadi string ISO 8601', () => {
      const dateStr = '2026-09-18T10:00:00.000Z';
      expect(cleanDate(dateStr)).toBe(dateStr);
      expect(cleanDate(new Date('2026-09-18T10:00:00.000Z'))).toBe(dateStr);
    });
  });

  describe('cleanCoordinate', () => {
    it('mengembalikan null untuk koordinat nol atau invalid', () => {
      expect(cleanCoordinate('0')).toBeNull();
      expect(cleanCoordinate('0.0')).toBeNull();
      expect(cleanCoordinate('0.000000')).toBeNull();
      expect(cleanCoordinate('N/A')).toBeNull();
      expect(cleanCoordinate('invalid')).toBeNull();
    });

    it('mempertahankan koordinat valid', () => {
      expect(cleanCoordinate('-6.200000')).toBe('-6.200000');
      expect(cleanCoordinate('106.816666')).toBe('106.816666');
    });
  });

  describe('cleanQueryCoordinate', () => {
    it('mengembalikan undefined untuk koordinat nol, invalid, null, atau dirty string', () => {
      expect(cleanQueryCoordinate(null)).toBeUndefined();
      expect(cleanQueryCoordinate(undefined)).toBeUndefined();
      expect(cleanQueryCoordinate('0')).toBeUndefined();
      expect(cleanQueryCoordinate('0.0')).toBeUndefined();
      expect(cleanQueryCoordinate('0.000000')).toBeUndefined();
      expect(cleanQueryCoordinate('N/A')).toBeUndefined();
      expect(cleanQueryCoordinate('invalid')).toBeUndefined();
    });

    it('mempertahankan koordinat valid', () => {
      expect(cleanQueryCoordinate('-6.200000')).toBe('-6.200000');
      expect(cleanQueryCoordinate('106.816666')).toBe('106.816666');
    });
  });

  describe('cleanNumber', () => {
    it('mengembalikan null untuk nilai null, undefined, NaN, Infinity, dirty strings, dan string non-numerik', () => {
      expect(cleanNumber(null)).toBeNull();
      expect(cleanNumber(undefined)).toBeNull();
      expect(cleanNumber(Number.NaN)).toBeNull();
      expect(cleanNumber(Number.POSITIVE_INFINITY)).toBeNull();
      expect(cleanNumber(Number.NEGATIVE_INFINITY)).toBeNull();
      expect(cleanNumber('')).toBeNull();
      expect(cleanNumber('   ')).toBeNull();
      expect(cleanNumber('N/A')).toBeNull();
      expect(cleanNumber('null')).toBeNull();
      expect(cleanNumber('abc')).toBeNull();
      expect(cleanNumber({})).toBeNull();
    });

    it('memvalidasi dan mempertahankan number bulat maupun pecahan desimal', () => {
      expect(cleanNumber(0)).toBe(0);
      expect(cleanNumber(42)).toBe(42);
      expect(cleanNumber(-17)).toBe(-17);
      expect(cleanNumber(12.75)).toBe(12.75);
      expect(cleanNumber(-3.5)).toBe(-3.5);
    });

    it('mengonversi string numerik menjadi number valid', () => {
      expect(cleanNumber('42')).toBe(42);
      expect(cleanNumber('  12.75  ')).toBe(12.75);
      expect(cleanNumber('-99.5')).toBe(-99.5);
    });
  });

  describe('cleanInteger', () => {
    it('mengembalikan null untuk nilai non-numerik atau invalid', () => {
      expect(cleanInteger(null)).toBeNull();
      expect(cleanInteger(undefined)).toBeNull();
      expect(cleanInteger('N/A')).toBeNull();
      expect(cleanInteger('invalid')).toBeNull();
      expect(cleanInteger(Number.NaN)).toBeNull();
      expect(cleanInteger(Number.POSITIVE_INFINITY)).toBeNull();
    });

    it('mempertahankan integer bulat', () => {
      expect(cleanInteger(0)).toBe(0);
      expect(cleanInteger(101)).toBe(101);
      expect(cleanInteger(-50)).toBe(-50);
      expect(cleanInteger('200')).toBe(200);
      expect(cleanInteger('  -35  ')).toBe(-35);
    });

    it('memotong komponen pecahan (Math.trunc) untuk menghasilkan integer murni', () => {
      expect(cleanInteger(12.75)).toBe(12);
      expect(cleanInteger('99.99')).toBe(99);
      expect(cleanInteger(-4.9)).toBe(-4);
    });
  });

  describe('cleanQueryInteger', () => {
    it('mengembalikan undefined untuk nilai non-numerik atau invalid', () => {
      expect(cleanQueryInteger(null)).toBeUndefined();
      expect(cleanQueryInteger(undefined)).toBeUndefined();
      expect(cleanQueryInteger('N/A')).toBeUndefined();
      expect(cleanQueryInteger('invalid')).toBeUndefined();
      expect(cleanQueryInteger(Number.NaN)).toBeUndefined();
      expect(cleanQueryInteger(Number.POSITIVE_INFINITY)).toBeUndefined();
    });

    it('mengembalikan integer murni untuk input numerik valid', () => {
      expect(cleanQueryInteger(0)).toBe(0);
      expect(cleanQueryInteger(101)).toBe(101);
      expect(cleanQueryInteger(-50)).toBe(-50);
      expect(cleanQueryInteger('200')).toBe(200);
      expect(cleanQueryInteger('  -35  ')).toBe(-35);
      expect(cleanQueryInteger(12.75)).toBe(12);
    });
  });

  describe('cleanBoolean & cleanQueryBoolean', () => {
    it('mengonversi boolean, string true/false/1/0 menjadi boolean valid', () => {
      expect(cleanBoolean(true)).toBe(true);
      expect(cleanBoolean(false)).toBe(false);
      expect(cleanBoolean('true')).toBe(true);
      expect(cleanBoolean('TRUE')).toBe(true);
      expect(cleanBoolean('  true  ')).toBe(true);
      expect(cleanBoolean('false')).toBe(false);
      expect(cleanBoolean('FALSE')).toBe(false);
      expect(cleanBoolean('1')).toBe(true);
      expect(cleanBoolean('0')).toBe(false);
      expect(cleanBoolean(1)).toBe(true);
      expect(cleanBoolean(0)).toBe(false);
    });

    it('mengembalikan null/undefined untuk nilai selain boolean', () => {
      expect(cleanBoolean(null)).toBeNull();
      expect(cleanBoolean(undefined)).toBeNull();
      expect(cleanBoolean('invalid')).toBeNull();
      expect(cleanQueryBoolean('invalid')).toBeUndefined();
      expect(cleanQueryBoolean(null)).toBeUndefined();
    });
  });

  describe('cleanUpdate* Mutation Utilities (Tri-State Semantics)', () => {
    describe('cleanUpdateString', () => {
      it('mengembalikan undefined jika input undefined (omitted)', () => {
        expect(cleanUpdateString(undefined)).toBeUndefined();
      });

      it('mengembalikan null jika input null, kosong, spasi, N/A, atau null literal (explicit clear)', () => {
        expect(cleanUpdateString(null)).toBeNull();
        expect(cleanUpdateString('')).toBeNull();
        expect(cleanUpdateString('   ')).toBeNull();
        expect(cleanUpdateString('N/A')).toBeNull();
        expect(cleanUpdateString('null')).toBeNull();
      });

      it('mengembalikan string ter-trim dan lowercase jika email valid', () => {
        expect(cleanUpdateString('  User@Example.com  ')).toBe('user@example.com');
        expect(cleanUpdateString('  Pos Satpam Utama  ')).toBe('Pos Satpam Utama');
      });
    });

    describe('cleanUpdateCoordinate', () => {
      it('mengembalikan undefined jika input undefined (omitted)', () => {
        expect(cleanUpdateCoordinate(undefined)).toBeUndefined();
      });

      it('mengembalikan null jika input null, kosong, 0, 0.0, 0.000000, N/A, atau NaN (clear)', () => {
        expect(cleanUpdateCoordinate(null)).toBeNull();
        expect(cleanUpdateCoordinate('')).toBeNull();
        expect(cleanUpdateCoordinate('   ')).toBeNull();
        expect(cleanUpdateCoordinate('0')).toBeNull();
        expect(cleanUpdateCoordinate('0.0')).toBeNull();
        expect(cleanUpdateCoordinate('0.000000')).toBeNull();
        expect(cleanUpdateCoordinate('N/A')).toBeNull();
        expect(cleanUpdateCoordinate('invalid-coord')).toBeNull();
      });

      it('mengembalikan string koordinat valid', () => {
        expect(cleanUpdateCoordinate('  -6.2088  ')).toBe('-6.2088');
        expect(cleanUpdateCoordinate('106.8456')).toBe('106.8456');
      });
    });

    describe('cleanUpdateDate', () => {
      it('mengembalikan undefined jika input undefined (omitted)', () => {
        expect(cleanUpdateDate(undefined)).toBeUndefined();
      });

      it('mengembalikan null jika input null, kosong, atau tanggal legacy epoch (clear)', () => {
        expect(cleanUpdateDate(null)).toBeNull();
        expect(cleanUpdateDate('')).toBeNull();
        expect(cleanUpdateDate('0000-00-00 00:00:00')).toBeNull();
        expect(cleanUpdateDate('1970-01-01 00:00:00')).toBeNull();
      });

      it('mengembalikan ISO string valid jika tanggal valid', () => {
        const dateStr = '2026-09-18T10:00:00.000Z';
        expect(cleanUpdateDate(dateStr)).toBe(dateStr);
      });
    });

    describe('cleanUpdateBoolean', () => {
      it('mengembalikan undefined jika input undefined', () => {
        expect(cleanUpdateBoolean(undefined)).toBeUndefined();
      });

      it('mengembalikan null jika input null atau invalid', () => {
        expect(cleanUpdateBoolean(null)).toBeNull();
        expect(cleanUpdateBoolean('invalid')).toBeNull();
      });

      it('mengembalikan boolean valid', () => {
        expect(cleanUpdateBoolean(true)).toBe(true);
        expect(cleanUpdateBoolean('false')).toBe(false);
        expect(cleanUpdateBoolean(1)).toBe(true);
        expect(cleanUpdateBoolean(0)).toBe(false);
      });
    });

    describe('cleanUpdateNumber & cleanUpdateInteger', () => {
      it('mengembalikan undefined jika input undefined', () => {
        expect(cleanUpdateNumber(undefined)).toBeUndefined();
        expect(cleanUpdateInteger(undefined)).toBeUndefined();
      });

      it('mengembalikan null jika input null atau invalid', () => {
        expect(cleanUpdateNumber(null)).toBeNull();
        expect(cleanUpdateNumber('NaN')).toBeNull();
        expect(cleanUpdateInteger(null)).toBeNull();
        expect(cleanUpdateInteger('invalid')).toBeNull();
      });

      it('mengembalikan number atau integer valid', () => {
        expect(cleanUpdateNumber(12.34)).toBe(12.34);
        expect(cleanUpdateInteger(12.75)).toBe(12);
        expect(cleanUpdateInteger('42')).toBe(42);
      });
    });
  });
});

