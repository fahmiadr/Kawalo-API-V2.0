import { AppConstants } from '@core/constants/app.constants';
import { cleanInteger } from '@core/utils/clean-data.util';

export interface PaginationParams {
  limit?: number;
  offset?: number;
}

export interface ResolvedPagination {
  /**
   * Menandakan apakah query diminta tanpa batas (limit === 0).
   */
  isNoLimit: boolean;
  /**
   * Jumlah data yang diambil untuk query Prisma / SQL. Bernilai `undefined` jika `isNoLimit` bernilai `true`.
   */
  take?: number;
  /**
   * Offset data untuk query Prisma / SQL. Bernilai `undefined` jika `isNoLimit` bernilai `true`.
   */
  skip?: number;
}

/**
 * Menyelesaikan dan menormalisasi parameter paginasi sistem.
 *
 * Aturan Paginasi:
 * 1. Jika `limit === 0`: Tanpa limit (`isNoLimit = true`), `take` dan `skip` bernilai `undefined`.
 * 2. Jika `limit` tidak diberikan atau invalid: Menggunakan `AppConstants.DEFAULT_PAGE_SIZE` (20).
 * 3. Jika `limit > 0`: Dibatasi secara aman antara 1 sampai `AppConstants.MAX_PAGE_SIZE` (100).
 * 4. `offset`: Divalidasi minimal 0.
 */
export function resolvePagination(params?: PaginationParams): ResolvedPagination {
  const cleanLimit = cleanInteger(params?.limit);
  if (cleanLimit === 0) {
    return {
      isNoLimit: true,
      take: undefined,
      skip: undefined,
    };
  }

  const rawLimit = cleanLimit ?? AppConstants.DEFAULT_PAGE_SIZE;
  const take = Math.min(Math.max(1, rawLimit), AppConstants.MAX_PAGE_SIZE);

  const rawOffset = cleanInteger(params?.offset) ?? 0;
  const skip = Math.max(0, rawOffset);

  return {
    isNoLimit: false,
    take,
    skip,
  };
}
