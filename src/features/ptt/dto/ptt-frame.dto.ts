import { type Static, t } from 'elysia';

export const PttSpeakerSchema = t.Object({
  userId: t.String(),
  name: t.String(),
  role: t.String(),
});
export type PttSpeakerDto = Static<typeof PttSpeakerSchema>;

/**
 * Inbound Floor Request (Client -> Server)
 * Cryptographic Security: No client-asserted 'role' allowed.
 * Role is strictly resolved server-side from JWT claims.
 */
export const FloorRequestFrameSchema = t.Object({
  type: t.Literal('FLOOR_REQUEST'),
  requestId: t.String({ minLength: 1 }),
});
export type FloorRequestFrameDto = Static<typeof FloorRequestFrameSchema>;

/**
 * Outbound Floor Granted (Server -> Requester Client)
 */
export const FloorGrantedFrameSchema = t.Object({
  type: t.Literal('FLOOR_GRANTED'),
  requestId: t.String(),
  leaseTtlMs: t.Number(),
});
export type FloorGrantedFrameDto = Static<typeof FloorGrantedFrameSchema>;

/**
 * Outbound Floor Denied (Server -> Requester Client)
 */
export const FloorDeniedFrameSchema = t.Object({
  type: t.Literal('FLOOR_DENIED'),
  requestId: t.String(),
  reason: t.Union([
    t.Literal('CHANNEL_BUSY'),
    t.Literal('INSUFFICIENT_PRIORITY'),
    t.Literal('RATE_LIMITED'),
  ]),
  currentSpeaker: t.Optional(PttSpeakerSchema),
});
export type FloorDeniedFrameDto = Static<typeof FloorDeniedFrameSchema>;

/**
 * Outbound Floor State Changed (Server -> All Room Subscribers)
 * Fully compliant with nullability requirements: speaker is null when state is IDLE.
 */
export const FloorStateChangedFrameSchema = t.Object({
  type: t.Literal('FLOOR_STATE_CHANGED'),
  state: t.Union([t.Literal('IDLE'), t.Literal('TRANSMITTING')]),
  speaker: t.Nullable(PttSpeakerSchema),
  memberCount: t.Number(),
});
export type FloorStateChangedFrameDto = Static<typeof FloorStateChangedFrameSchema>;

/**
 * Inbound Floor Released (Client -> Server)
 */
export const FloorReleasedFrameSchema = t.Object({
  type: t.Literal('FLOOR_RELEASED'),
  reason: t.Optional(t.String()),
});
export type FloorReleasedFrameDto = Static<typeof FloorReleasedFrameSchema>;

/**
 * Outbound Floor Revoked (Server -> Preempted Speaker Client)
 */
export const FloorRevokedFrameSchema = t.Object({
  type: t.Literal('FLOOR_REVOKED'),
  reason: t.Union([
    t.Literal('ADMIN_OVERRIDE'),
    t.Literal('TIME_OUT_TIMER_EXPIRED'),
    t.Literal('CONNECTION_LOST'),
  ]),
  preemptedBy: t.Optional(t.String()),
});
export type FloorRevokedFrameDto = Static<typeof FloorRevokedFrameSchema>;

/**
 * Outbound Floor Reset / Queue Purge (Server -> All Listening Clients)
 */
export const FloorResetFrameSchema = t.Object({
  type: t.Literal('FLOOR_RESET'),
  reason: t.Literal('PREEMPTION_FLUSH'),
});
export type FloorResetFrameDto = Static<typeof FloorResetFrameSchema>;

/**
 * Outbound Member Count Updated (Server -> All Room Subscribers)
 */
export const MemberCountUpdatedFrameSchema = t.Object({
  type: t.Literal('MEMBER_COUNT_UPDATED'),
  memberCount: t.Number(),
});
export type MemberCountUpdatedFrameDto = Static<typeof MemberCountUpdatedFrameSchema>;

/**
 * Inbound Control Frame Union Schema
 */
export const InboundPttControlFrameSchema = t.Union([
  FloorRequestFrameSchema,
  FloorReleasedFrameSchema,
]);
export type InboundPttControlFrameDto = Static<typeof InboundPttControlFrameSchema>;
