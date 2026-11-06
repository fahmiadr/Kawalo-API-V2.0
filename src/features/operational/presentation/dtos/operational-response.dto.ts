import { type Static, t } from 'elysia';

export const OperationalTimelineItemDto = t.Object({
  id: t.String(),
  journey_uid: t.Nullable(t.String()),
  duty: t.Nullable(t.String()),
  state: t.Nullable(t.String()),
  duty_dt: t.Nullable(t.String()),
  state_dt: t.Nullable(t.String()),
});

export type OperationalTimelineItemDtoType = Static<typeof OperationalTimelineItemDto>;

export const OperationalItemDto = t.Object({
  id: t.String(),
  user_email: t.String(),
  user_name: t.Nullable(t.String()),
  user_profile_pic: t.Nullable(t.String()),
  group_name: t.Nullable(t.String()),
  latest_duty: t.Nullable(t.String()),
  latest_state: t.Nullable(t.String()),
  latest_duty_dt: t.Nullable(t.String()),
  latest_state_dt: t.Nullable(t.String()),
  total_activities: t.Number(),
  duty_date: t.Nullable(t.String()),
  timeline: t.Array(OperationalTimelineItemDto),
});

export type OperationalItemDtoType = Static<typeof OperationalItemDto>;

export const GetOperationalResponseDto = t.Array(OperationalItemDto);
export type GetOperationalResponseDtoType = Static<typeof GetOperationalResponseDto>;
