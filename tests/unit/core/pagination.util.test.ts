import { AppConstants } from '@core/constants/app.constants';
import { resolvePagination } from '@core/utils/pagination.util';
import { describe, expect, it } from 'vitest';

describe('PaginationUtil Unit Tests', () => {
  it('harus mengembalikan isNoLimit = true dan undefined take/skip saat limit = 0', () => {
    const result = resolvePagination({ limit: 0, offset: 10 });
    expect(result.isNoLimit).toBe(true);
    expect(result.take).toBeUndefined();
    expect(result.skip).toBeUndefined();
  });

  it('harus menggunakan DEFAULT_PAGE_SIZE saat limit tidak ditentukan', () => {
    const result = resolvePagination();
    expect(result.isNoLimit).toBe(false);
    expect(result.take).toBe(AppConstants.DEFAULT_PAGE_SIZE);
    expect(result.skip).toBe(0);
  });

  it('harus menerima limit dan offset yang valid', () => {
    const result = resolvePagination({ limit: 50, offset: 15 });
    expect(result.isNoLimit).toBe(false);
    expect(result.take).toBe(50);
    expect(result.skip).toBe(15);
  });

  it('harus membatasi limit maksimal ke MAX_PAGE_SIZE (100)', () => {
    const result = resolvePagination({ limit: 999 });
    expect(result.isNoLimit).toBe(false);
    expect(result.take).toBe(AppConstants.MAX_PAGE_SIZE);
  });

  it('harus membatasi limit minimal ke 1 jika bernilai negatif', () => {
    const result = resolvePagination({ limit: -10 });
    expect(result.isNoLimit).toBe(false);
    expect(result.take).toBe(1);
  });

  it('harus membatasi offset minimal ke 0 jika bernilai negatif', () => {
    const result = resolvePagination({ limit: 10, offset: -5 });
    expect(result.isNoLimit).toBe(false);
    expect(result.take).toBe(10);
    expect(result.skip).toBe(0);
  });

  it('harus menangani nilai NaN pada limit dan offset dengan fallback aman', () => {
    const result = resolvePagination({ limit: Number.NaN, offset: Number.NaN });
    expect(result.isNoLimit).toBe(false);
    expect(result.take).toBe(AppConstants.DEFAULT_PAGE_SIZE);
    expect(result.skip).toBe(0);
  });

  it('harus memotong komponen pecahan desimal pada limit dan offset secara aman', () => {
    const result = resolvePagination({ limit: 25.8, offset: 12.3 });
    expect(result.isNoLimit).toBe(false);
    expect(result.take).toBe(25);
    expect(result.skip).toBe(12);
  });
});
