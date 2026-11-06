import { Failure } from '@core/error/failure';
import { Result } from '@core/error/result';
import { describe, expect, it } from 'vitest';

describe('Result<T, E> Monad Unit Tests', () => {
  describe('Basic Constructors & Accessors', () => {
    it('Result.ok harus menandakan keberhasilan dan mengembalikan nilai', () => {
      const result = Result.ok<string>('hello world');

      expect(result.isSuccess()).toBe(true);
      expect(result.isFailure()).toBe(false);
      expect(result.value).toBe('hello world');
    });

    it('Result.ok harus melempar error jika mengakses .failure', () => {
      const result = Result.ok<number>(42);

      expect(() => result.failure).toThrow(
        'Cannot retrieve failure from a successful Result. Check isFailure() first.',
      );
    });

    it('Result.fail harus menandakan kegagalan dan mengembalikan failure', () => {
      const failure = Failure.notFound('Data tidak ditemukan');
      const result = Result.fail<string>(failure);

      expect(result.isSuccess()).toBe(false);
      expect(result.isFailure()).toBe(true);
      expect(result.failure).toBe(failure);
      expect(result.failure.errorCode).toBe('RESOURCE_NOT_FOUND');
    });

    it('Result.fail harus melempar error jika mengakses .value', () => {
      const failure = Failure.server('Database error');
      const result = Result.fail<string>(failure);

      expect(() => result.value).toThrow(
        'Cannot retrieve value from a failed Result. Check isSuccess() first.',
      );
    });
  });

  describe('fold (Pattern Matching)', () => {
    it('harus memanggil onSuccess ketika result sukses', () => {
      const result = Result.ok<number>(100);

      const message = result.fold(
        (val) => `Success: ${val * 2}`,
        (fail) => `Failed: ${fail.message}`,
      );

      expect(message).toBe('Success: 200');
    });

    it('harus memanggil onFailure ketika result gagal', () => {
      const failure = Failure.forbidden('Akses ditolak');
      const result = Result.fail<number>(failure);

      const message = result.fold(
        (val) => `Success: ${val * 2}`,
        (fail) => `Failed: ${fail.message}`,
      );

      expect(message).toBe('Failed: Akses ditolak');
    });
  });

  describe('map (Value Transformation)', () => {
    it('harus mentransformasi nilai jika result sukses', () => {
      const result = Result.ok<number>(21);
      const mapped = result.map((n) => n * 2);

      expect(mapped.isSuccess()).toBe(true);
      expect(mapped.value).toBe(42);
    });

    it('harus meneruskan failure tanpa memanggil transformasi jika result gagal', () => {
      const failure = Failure.validation('Invalid input');
      const result = Result.fail<number>(failure);

      let called = false;
      const mapped = result.map((n) => {
        called = true;
        return n * 2;
      });

      expect(called).toBe(false);
      expect(mapped.isFailure()).toBe(true);
      expect(mapped.failure).toBe(failure);
    });
  });

  describe('flatMap (Monadic Chaining)', () => {
    it('harus merangkai operasi sukses ke Result berikutnya', () => {
      const parseNumber = (s: string): Result<number> => {
        const num = Number(s);
        return Number.isFinite(num)
          ? Result.ok(num)
          : Result.fail(Failure.validation('Not a number'));
      };

      const result = Result.ok<string>('123').flatMap(parseNumber);

      expect(result.isSuccess()).toBe(true);
      expect(result.value).toBe(123);
    });

    it('harus menghentikan rantai dan meneruskan failure jika step awal gagal', () => {
      const initialFailure = Failure.auth('Unauthorized');
      const result = Result.fail<string>(initialFailure).flatMap((s) => Result.ok(s.length));

      expect(result.isFailure()).toBe(true);
      expect(result.failure).toBe(initialFailure);
    });
  });

  describe('getOrElse (Safe Value Extraction)', () => {
    it('harus mengembalikan nilai asli jika Result sukses', () => {
      const result = Result.ok<string>('actual-value');
      expect(result.getOrElse('default-value')).toBe('actual-value');
    });

    it('harus mengembalikan fallback defaultValue jika Result gagal', () => {
      const result = Result.fail<string>(Failure.server('Server Down'));
      expect(result.getOrElse('fallback-value')).toBe('fallback-value');
    });
  });
});
