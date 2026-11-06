import { type Static, t } from 'elysia';

export const MasterGuestTypeItemDto = t.Object({
  id: t.String({ description: 'ID Master Tipe Tamu' }),
  uid: t.String({ description: 'UID Master Tipe Tamu' }),
  name: t.String({ description: 'Nama Master Tipe Tamu' }),
});

export type MasterGuestTypeItemDtoType = Static<typeof MasterGuestTypeItemDto>;

export const GetMasterGuestTypesResponseDto = t.Array(MasterGuestTypeItemDto);
export type GetMasterGuestTypesResponseDtoType = Static<typeof GetMasterGuestTypesResponseDto>;

export const GuestItemDto = t.Object({
  id: t.String(),
  group_name: t.Nullable(t.String()),
  guest_name: t.Nullable(t.String()),
  guest_number: t.Nullable(t.String()),
  guest_identity: t.Nullable(t.String()),
  guest_identity_type: t.Nullable(t.String()),
  guest_photo: t.Nullable(t.String()),
  guest_dt: t.Nullable(t.String()),
  guest_ack_dt: t.Nullable(t.String()),
  guest_out_dt: t.Nullable(t.String()),
  guest_purpose: t.Nullable(t.String()),
  guest_destination: t.Nullable(t.String()),
  guest_to_user_email: t.Nullable(t.String()),
  guest_to_user_name: t.Nullable(t.String()),
  guest_creator_email: t.Nullable(t.String()),
  guest_creator_name: t.Nullable(t.String()),
  guest_category: t.Nullable(t.String()),
  qr_uid: t.Nullable(t.String()),
  guest_address: t.Nullable(t.String()),
  guest_type: t.Nullable(t.String()),
  guest_checkout_by_email: t.Nullable(t.String()),
  guest_checkout_by_name: t.Nullable(t.String()),
});

export type GuestItemDtoType = Static<typeof GuestItemDto>;

export const GetGuestsResponseDto = t.Array(GuestItemDto);
export type GetGuestsResponseDtoType = Static<typeof GetGuestsResponseDto>;

export const DeleteBatchGuestResponseDto = t.Object({
  deleted_count: t.Integer({
    minimum: 0,
    description: 'Jumlah record tamu yang berhasil dihapus',
  }),
});

export type DeleteBatchGuestResponseDtoType = Static<typeof DeleteBatchGuestResponseDto>;
