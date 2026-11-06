import { type Static, t } from 'elysia';

export const AttendanceItemDto = t.Object({
  id: t.String(),
  user_email: t.Nullable(t.String()),
  user_name: t.Nullable(t.String()),
  user_profile_pic: t.Nullable(t.String()),
  user_duty: t.Nullable(t.String()),
  attendance_dt: t.Nullable(t.String()),
  attendance_type: t.Nullable(t.String()),
  category: t.String(),
  latitude: t.Nullable(t.String()),
  longitude: t.Nullable(t.String()),
  diff_minutes: t.Number(),
  location_name: t.Nullable(t.String()),
  shift_name: t.Nullable(t.String()),
  shift_color: t.Nullable(t.String()),
  shift_time_window: t.Nullable(t.String()),
  group_name: t.Nullable(t.String()),
});

export type AttendanceItemDtoType = Static<typeof AttendanceItemDto>;

export const GetAttendanceResponseDto = t.Array(AttendanceItemDto);
export type GetAttendanceResponseDtoType = Static<typeof GetAttendanceResponseDto>;
