import { type Static, t } from 'elysia';

export const InformationItemDto = t.Object({
  id: t.String(),
  title: t.Nullable(t.String()),
  text: t.Nullable(t.String()),
  user_email: t.Nullable(t.String()),
  user_name: t.Nullable(t.String()),
  user_profile_pic: t.Nullable(t.String()),
  feed_dt: t.Nullable(t.String()),
  feed_file: t.Nullable(t.String()),
  photos: t.Array(t.String()),
  feed_type: t.Nullable(t.String()),
  group_name: t.Nullable(t.String()),
  event_location: t.Nullable(t.String()),
  event_start_dt: t.Nullable(t.String()),
  event_end_dt: t.Nullable(t.String()),
  event_duration: t.Nullable(t.Number()),
  glat: t.Nullable(t.String()),
  glong: t.Nullable(t.String()),
});

export type InformationItemDtoType = Static<typeof InformationItemDto>;

export const GetInformationResponseDto = t.Array(InformationItemDto);
export type GetInformationResponseDtoType = Static<typeof GetInformationResponseDto>;

export const DeleteBatchInformationResponseDto = t.Object({
  deleted_count: t.Integer({
    minimum: 0,
    description: 'Jumlah data informasi yang berhasil dihapus',
  }),
});
export type DeleteBatchInformationResponseDtoType = Static<
  typeof DeleteBatchInformationResponseDto
>;
