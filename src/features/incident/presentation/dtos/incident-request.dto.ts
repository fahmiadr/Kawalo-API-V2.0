import { AppConstants } from '@core/constants/app.constants';
import { type Static, t } from 'elysia';

const photos = t.Array(t.String({ maxLength: 2048 }), {
  maxItems: 4,
  description: 'Daftar URL foto (maks 4 item)',
});

export const IncidentIdParamsDto = t.Object({
  id: t.String({ minLength: 1, maxLength: 64, description: 'ID insiden (IncidentUID)' }),
});
export type IncidentIdParamsDtoType = Static<typeof IncidentIdParamsDto>;

export const GetIncidentsQueryDto = t.Object({
  status: t.Optional(t.String({ description: 'OPEN atau RESOLVED' })),
  search: t.Optional(t.String({ minLength: 1, description: 'Kata kunci judul/deskripsi/pelapor' })),
  start_date: t.Optional(t.String({ description: 'Filter tanggal mulai (ISO 8601)' })),
  end_date: t.Optional(t.String({ description: 'Filter tanggal akhir (ISO 8601)' })),
  limit: t.Optional(
    t.Numeric({
      minimum: 0,
      maximum: AppConstants.MAX_PAGE_SIZE,
      default: AppConstants.DEFAULT_PAGE_SIZE,
      description: 'Jumlah data per halaman (0 untuk tanpa limit)',
    }),
  ),
  offset: t.Optional(t.Numeric({ minimum: 0, default: 0, description: 'Offset paginasi' })),
});
export type GetIncidentsQueryDtoType = Static<typeof GetIncidentsQueryDto>;

export const CreateIncidentDto = t.Object({
  title: t.String({ minLength: 1, maxLength: 128 }),
  // group_id dikirim app tetapi diabaikan: grup selalu diambil dari token (isolasi tenant).
  group_id: t.Optional(t.String()),
  description: t.Optional(t.Nullable(t.String({ maxLength: 1024 }))),
  category: t.Optional(t.Nullable(t.String({ maxLength: 16 }))),
  glat: t.Optional(t.Nullable(t.Number({ minimum: -90, maximum: 90 }))),
  glong: t.Optional(t.Nullable(t.Number({ minimum: -180, maximum: 180 }))),
  photos: t.Optional(photos),
});
export type CreateIncidentDtoType = Static<typeof CreateIncidentDto>;

export const PostIncidentReplyDto = t.Object({
  message: t.String({ maxLength: 2000 }),
  photos: t.Optional(photos),
});
export type PostIncidentReplyDtoType = Static<typeof PostIncidentReplyDto>;
