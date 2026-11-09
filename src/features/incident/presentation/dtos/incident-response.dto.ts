import { t } from 'elysia';

export const IncidentItemDto = t.Object({
  id: t.String(),
  group_id: t.String(),
  title: t.String(),
  description: t.Nullable(t.String()),
  category: t.String(),
  glat: t.Nullable(t.Number()),
  glong: t.Nullable(t.Number()),
  photos: t.Array(t.String()),
  status: t.Union([t.Literal('OPEN'), t.Literal('RESOLVED')]),
  created_by: t.String(),
  created_by_name: t.Nullable(t.String()),
  resolved_by: t.Nullable(t.String()),
  resolved_by_name: t.Nullable(t.String()),
  resolved_at: t.Nullable(t.String()),
  deleted_at: t.Nullable(t.String()),
  created_at: t.String(),
  updated_at: t.String(),
});

export const IncidentReplyItemDto = t.Object({
  id: t.String(),
  incident_id: t.String(),
  sender_id: t.String(),
  sender_name: t.String(),
  sender_role: t.String(),
  message: t.String(),
  photos: t.Array(t.String()),
  created_at: t.String(),
});

export const IncidentListResponseDto = t.Array(IncidentItemDto);

export const IncidentDetailResponseDto = t.Object({
  incident: IncidentItemDto,
  replies: t.Array(IncidentReplyItemDto),
});
