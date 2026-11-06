import { AppConstants } from '@core/constants/app.constants';
import { type Static, t } from 'elysia';

export const GetOperationalQueryDto = t.Object({
  search: t.Optional(
    t.String({
      minLength: 1,
      description: 'Kata kunci pencarian nama, email, tugas, atau status',
    }),
  ),
  limit: t.Optional(
    t.Numeric({
      minimum: 0,
      maximum: AppConstants.MAX_PAGE_SIZE,
      default: AppConstants.DEFAULT_PAGE_SIZE,
      description: 'Jumlah data per halaman (0 untuk tanpa limit)',
    }),
  ),
  offset: t.Optional(
    t.Numeric({
      minimum: 0,
      default: 0,
      description: 'Offset data paginasi',
    }),
  ),
  start_date: t.Optional(t.String({ description: 'Filter tanggal mulai (ISO 8601)' })),
  end_date: t.Optional(t.String({ description: 'Filter tanggal akhir (ISO 8601)' })),
});

export type GetOperationalQueryDtoType = Static<typeof GetOperationalQueryDto>;
