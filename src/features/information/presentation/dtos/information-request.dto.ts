import { AppConstants } from '@core/constants/app.constants';
import { type Static, t } from 'elysia';

export const GetInformationQueryDto = t.Object({
  search: t.Optional(
    t.String({
      minLength: 1,
      description: 'Kata kunci pencarian judul, deskripsi, pelapor, lokasi, atau tipe',
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
  created_by: t.Optional(
    t.String({
      minLength: 1,
      description: 'Filter berdasarkan email pembuat informasi',
    }),
  ),
});

export type GetInformationQueryDtoType = Static<typeof GetInformationQueryDto>;

export const InformationIdParamsDto = t.Object({
  id: t.String({
    pattern: '^[0-9]+$',
    description: 'ID Informasi numerik (FeedId)',
  }),
});

export type InformationIdParamsDtoType = Static<typeof InformationIdParamsDto>;

export const InformationFeedTypeEnum = t.Union([
  t.Literal('GENERAL'),
  t.Literal('JOIN'),
  t.Literal('INCIDENT'),
  t.Literal('EVENT'),
  t.Literal('OTHER'),
]);

export const CreateInformationDto = t.Object({
  title: t.String({ minLength: 1, maxLength: 128, description: 'Judul informasi (FeedTitle)' }),
  text: t.Optional(
    t.Nullable(t.String({ maxLength: 1024, description: 'Isi teks informasi (FeedText)' })),
  ),
  feed_file: t.Optional(
    t.Nullable(t.String({ maxLength: 2048, description: 'File lampiran URL (FeedFile)' })),
  ),
  photos: t.Optional(
    t.Array(t.String({ maxLength: 2048, description: 'Daftar URL/nama foto' }), {
      maxItems: 4,
      description: 'Daftar URL foto (FeedPic s/d FeedPic4, maks 4 item)',
    }),
  ),
  feed_type: t.Optional(
    t.Nullable(InformationFeedTypeEnum, {
      default: 'OTHER',
      description: 'Tipe informasi (GENERAL, JOIN, INCIDENT, EVENT, OTHER - default: OTHER)',
    }),
  ),
  event_location: t.Optional(
    t.Nullable(
      t.String({ maxLength: 128, description: 'Lokasi acara/kegiatan (FeedEventLocation)' }),
    ),
  ),
  glat: t.Optional(t.Nullable(t.String({ maxLength: 32, description: 'Latitude lokasi (GLat)' }))),
  glong: t.Optional(
    t.Nullable(t.String({ maxLength: 32, description: 'Longitude lokasi (GLong)' })),
  ),
});

export type CreateInformationDtoType = Static<typeof CreateInformationDto>;

export const UpdateInformationDto = t.Object({
  title: t.Optional(
    t.Nullable(
      t.String({ minLength: 1, maxLength: 128, description: 'Judul informasi (FeedTitle)' }),
    ),
  ),
  text: t.Optional(
    t.Nullable(t.String({ maxLength: 1024, description: 'Isi teks informasi (FeedText)' })),
  ),
  feed_file: t.Optional(
    t.Nullable(t.String({ maxLength: 2048, description: 'File lampiran URL (FeedFile)' })),
  ),
  photos: t.Optional(
    t.Array(t.String({ maxLength: 2048, description: 'Daftar URL/nama foto' }), {
      maxItems: 4,
      description: 'Daftar URL foto (FeedPic s/d FeedPic4, maks 4 item)',
    }),
  ),
  feed_type: t.Optional(
    t.Nullable(InformationFeedTypeEnum, {
      description: 'Tipe informasi (GENERAL, JOIN, INCIDENT, EVENT, OTHER)',
    }),
  ),
  event_location: t.Optional(
    t.Nullable(
      t.String({ maxLength: 128, description: 'Lokasi acara/kegiatan (FeedEventLocation)' }),
    ),
  ),
  glat: t.Optional(t.Nullable(t.String({ maxLength: 32, description: 'Latitude lokasi (GLat)' }))),
  glong: t.Optional(
    t.Nullable(t.String({ maxLength: 32, description: 'Longitude lokasi (GLong)' })),
  ),
});

export type UpdateInformationDtoType = Static<typeof UpdateInformationDto>;

export const DeleteBatchInformationDto = t.Object({
  ids: t.Array(
    t.String({
      pattern: '^[0-9]+$',
      description: 'ID Informasi numerik (FeedId)',
    }),
    {
      minItems: 1,
      description: 'Daftar ID informasi numerik yang akan dihapus',
    },
  ),
});

export type DeleteBatchInformationDtoType = Static<typeof DeleteBatchInformationDto>;
