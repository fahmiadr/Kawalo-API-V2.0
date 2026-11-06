import { AppConstants } from '@core/constants/app.constants';
import { type Static, t } from 'elysia';

export const GetGuestsQueryDto = t.Object({
  search: t.Optional(
    t.String({
      minLength: 1,
      description:
        'Kata kunci pencarian nama, nomor plat, identitas, tujuan, keperluan, kode QR, penerima tamu, atau petugas checkout',
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
      description: 'Filter berdasarkan email pembuat data tamu',
    }),
  ),
  is_active: t.Optional(
    t.Union([t.Boolean(), t.BooleanString()], {
      description: 'Filter tamu aktif: true (belum checkout), false (sudah checkout)',
    }),
  ),
});

export type GetGuestsQueryDtoType = Static<typeof GetGuestsQueryDto>;

export const GuestIdParamsDto = t.Object({
  id: t.String({
    minLength: 1,
    maxLength: 64,
    pattern: '^[a-zA-Z0-9_-]+$',
    description: 'ID Tamu numerik (GuestId) atau QRUID',
  }),
});

export type GuestIdParamsDtoType = Static<typeof GuestIdParamsDto>;

export const GuestIdentityTypeEnum = t.Union([
  t.Literal('KTP'),
  t.Literal('SIM'),
  t.Literal('PASPOR'),
  t.Literal('LAINNYA'),
]);

export type GuestIdentityTypeEnumType = Static<typeof GuestIdentityTypeEnum>;

export const CreateGuestDto = t.Object({
  guest_name: t.String({ minLength: 1, maxLength: 128, description: 'Nama tamu' }),
  guest_destination: t.String({ minLength: 1, maxLength: 128, description: 'Tujuan/lokasi tamu' }),
  guest_purpose: t.Optional(
    t.Nullable(t.String({ maxLength: 128, description: 'Keperluan kunjungan' })),
  ),
  guest_number: t.Optional(
    t.Nullable(t.String({ maxLength: 32, description: 'Nomor plat kendaraan' })),
  ),
  guest_identity: t.Optional(
    t.Nullable(t.String({ maxLength: 64, description: 'Nomor identitas (KTP/SIM)' })),
  ),
  guest_identity_type: t.Optional(t.Nullable(GuestIdentityTypeEnum)),
  guest_type: t.String({
    minLength: 1,
    maxLength: 64,
    description: 'Tipe tamu (harus terdaftar di tabel tkmasterguesttype grup aktif)',
  }),
  guest_address: t.Optional(
    t.Nullable(t.String({ maxLength: 512, description: 'Alamat asal tamu' })),
  ),
  guest_to_user_email: t.Optional(
    t.Nullable(
      t.String({ format: 'email', maxLength: 128, description: 'Email penghuni yang dituju' }),
    ),
  ),
  guest_photo: t.Optional(
    t.Nullable(t.String({ maxLength: 2048, description: 'Foto tamu / kendaraan (URL)' })),
  ),
  guest_dt: t.Optional(t.Nullable(t.String({ description: 'Waktu kedatangan (ISO 8601)' }))),
});

export type CreateGuestDtoType = Static<typeof CreateGuestDto>;

export const UpdateGuestDto = t.Object({
  guest_name: t.Optional(t.String({ minLength: 1, maxLength: 128, description: 'Nama tamu' })),
  guest_destination: t.Optional(
    t.String({ minLength: 1, maxLength: 128, description: 'Tujuan/lokasi tamu' }),
  ),
  guest_purpose: t.Optional(
    t.Nullable(t.String({ maxLength: 128, description: 'Keperluan kunjungan' })),
  ),
  guest_number: t.Optional(
    t.Nullable(t.String({ maxLength: 32, description: 'Nomor plat kendaraan' })),
  ),
  guest_identity: t.Optional(
    t.Nullable(t.String({ maxLength: 64, description: 'Nomor identitas' })),
  ),
  guest_identity_type: t.Optional(t.Nullable(GuestIdentityTypeEnum)),
  guest_type: t.Optional(t.String({ minLength: 1, maxLength: 64, description: 'Tipe tamu' })),
  guest_address: t.Optional(
    t.Nullable(t.String({ maxLength: 512, description: 'Alamat asal tamu' })),
  ),
  guest_to_user_email: t.Optional(
    t.Nullable(
      t.String({ format: 'email', maxLength: 128, description: 'Email penghuni yang dituju' }),
    ),
  ),
  guest_photo: t.Optional(
    t.Nullable(t.String({ maxLength: 2048, description: 'Foto tamu (URL)' })),
  ),
});

export type UpdateGuestDtoType = Static<typeof UpdateGuestDto>;

export const AcknowledgeGuestDto = t.Object({
  qr_uid: t.Optional(
    t.Nullable(
      t.String({
        maxLength: 64,
        description: 'Kode QR tamu (opsional jika dikirim via URL path)',
      }),
    ),
  ),
});

export type AcknowledgeGuestDtoType = Static<typeof AcknowledgeGuestDto>;

export const CheckoutGuestDto = t.Object({
  qr_uid: t.Optional(
    t.Nullable(
      t.String({
        maxLength: 64,
        description: 'Kode QR tamu (opsional jika dikirim via URL path)',
      }),
    ),
  ),
});

export type CheckoutGuestDtoType = Static<typeof CheckoutGuestDto>;

export const DeleteBatchGuestDto = t.Object({
  ids: t.Array(
    t.String({
      pattern: '^[0-9]+$',
      description: 'ID Tamu numerik (GuestId)',
    }),
    {
      minItems: 1,
      description: 'Daftar ID tamu numerik yang akan dihapus',
    },
  ),
});

export type DeleteBatchGuestDtoType = Static<typeof DeleteBatchGuestDto>;
