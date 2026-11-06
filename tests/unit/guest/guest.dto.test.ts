import {
  AcknowledgeGuestDto,
  CheckoutGuestDto,
  CreateGuestDto,
  GetGuestsQueryDto,
  GuestIdParamsDto,
  UpdateGuestDto,
} from '@features/guest/presentation/dtos/guest-request.dto';
import { Value } from '@sinclair/typebox/value';
import { describe, expect, it } from 'vitest';

describe('Guest DTO Schema Validation Unit Tests', () => {
  describe('GuestIdParamsDto', () => {
    it('menerima ID numerik dan kode QR/UUID yang valid', () => {
      expect(Value.Check(GuestIdParamsDto, { id: '123' })).toBe(true);
      expect(Value.Check(GuestIdParamsDto, { id: '1' })).toBe(true);
      expect(Value.Check(GuestIdParamsDto, { id: 'QR-VER-001' })).toBe(true);
      expect(Value.Check(GuestIdParamsDto, { id: 'c3d2b21b-4179-4d69-b541-6156e71ef3b5' })).toBe(
        true,
      );
    });

    it('menolak ID kosong atau string dengan spasi/simbol tidak sah', () => {
      expect(Value.Check(GuestIdParamsDto, { id: '' })).toBe(false);
      expect(Value.Check(GuestIdParamsDto, { id: 'id with spaces' })).toBe(false);
      expect(Value.Check(GuestIdParamsDto, { id: 'id@invalid#special' })).toBe(false);
    });
  });

  describe('AcknowledgeGuestDto & CheckoutGuestDto', () => {
    it('menerima payload konfirmasi dan checkout dengan qr_uid opsional', () => {
      expect(Value.Check(AcknowledgeGuestDto, { qr_uid: 'QR-001' })).toBe(true);
      expect(Value.Check(AcknowledgeGuestDto, { qr_uid: null })).toBe(true);
      expect(Value.Check(AcknowledgeGuestDto, {})).toBe(true);

      expect(Value.Check(CheckoutGuestDto, { qr_uid: 'QR-001' })).toBe(true);
      expect(Value.Check(CheckoutGuestDto, { qr_uid: null })).toBe(true);
      expect(Value.Check(CheckoutGuestDto, {})).toBe(true);
    });
  });

  describe('CreateGuestDto', () => {
    it('menerima payload valid sesuai batasan kolom DB', () => {
      const validPayload = {
        guest_name: 'Budi Santoso',
        guest_destination: 'Blok A1 No. 5',
        guest_type: 'TAMU',
        guest_number: 'B 1234 CD',
        guest_identity: '3201234567890001',
        guest_identity_type: 'KTP',
        guest_purpose: 'Kunjungan keluarga',
        guest_address: 'Jl. Melati No. 10, Jakarta Selatan',
        guest_to_user_email: 'resident@veranda.com',
      };
      expect(Value.Check(CreateGuestDto, validPayload)).toBe(true);
    });

    it('menerima guest_name hingga 128 karakter dan menolak jika melebihi 128 karakter', () => {
      const validPayload = {
        guest_name: 'A'.repeat(128),
        guest_destination: 'Blok A1',
        guest_type: 'TAMU',
      };
      expect(Value.Check(CreateGuestDto, validPayload)).toBe(true);

      const invalidPayload = {
        guest_name: 'A'.repeat(129),
        guest_destination: 'Blok A1',
        guest_type: 'TAMU',
      };
      expect(Value.Check(CreateGuestDto, invalidPayload)).toBe(false);
    });

    it('menolak guest_destination yang melebihi 128 karakter (VarChar 128)', () => {
      const invalidPayload = {
        guest_name: 'Budi',
        guest_destination: 'D'.repeat(129),
        guest_type: 'TAMU',
      };
      expect(Value.Check(CreateGuestDto, invalidPayload)).toBe(false);
    });

    it('menerima guest_type hingga 64 karakter dan menolak jika melebihi 64 karakter (VarChar 64)', () => {
      const validPayload = {
        guest_name: 'Budi',
        guest_destination: 'Blok A1',
        guest_type: 'T'.repeat(64),
      };
      expect(Value.Check(CreateGuestDto, validPayload)).toBe(true);

      const invalidPayload = {
        guest_name: 'Budi',
        guest_destination: 'Blok A1',
        guest_type: 'T'.repeat(65),
      };
      expect(Value.Check(CreateGuestDto, invalidPayload)).toBe(false);
    });

    it('menolak guest_number yang melebihi 32 karakter (VarChar 32)', () => {
      const invalidPayload = {
        guest_name: 'Budi',
        guest_destination: 'Blok A1',
        guest_type: 'TAMU',
        guest_number: 'B'.repeat(33),
      };
      expect(Value.Check(CreateGuestDto, invalidPayload)).toBe(false);
    });

    it('menolak format email yang tidak valid pada guest_to_user_email', () => {
      const invalidPayload = {
        guest_name: 'Budi',
        guest_destination: 'Blok A1',
        guest_type: 'TAMU',
        guest_to_user_email: 'bukan-email-valid',
      };
      expect(Value.Check(CreateGuestDto, invalidPayload)).toBe(false);
    });

    it('menolak email yang melebihi 128 karakter (VarChar 128)', () => {
      const invalidPayload = {
        guest_name: 'Budi',
        guest_destination: 'Blok A1',
        guest_type: 'TAMU',
        guest_to_user_email: `${'a'.repeat(120)}@domain.com`, // > 128 chars
      };
      expect(Value.Check(CreateGuestDto, invalidPayload)).toBe(false);
    });

    it('menerima guest_photo berupa URL standar yang lebih panjang dari 64 karakter', () => {
      const validPayload = {
        guest_name: 'Budi',
        guest_destination: 'Blok A1',
        guest_type: 'TAMU',
        guest_photo:
          'https://storage.googleapis.com/kawalo-bucket/guests/photos/2026/09/23/very-long-unique-uuid-image-name-avatar-photo-sample-123456789.jpg',
      };
      expect(Value.Check(CreateGuestDto, validPayload)).toBe(true);
    });

    it('menolak guest_photo yang melebihi 2048 karakter (VarChar 2048)', () => {
      const invalidPayload = {
        guest_name: 'Budi',
        guest_destination: 'Blok A1',
        guest_type: 'TAMU',
        guest_photo: `https://example.com/${'a'.repeat(2040)}`, // > 2048 chars
      };
      expect(Value.Check(CreateGuestDto, invalidPayload)).toBe(false);
    });

    it('hanya menerima guest_identity_type KTP, SIM, PASPOR, LAINNYA, atau null', () => {
      const basePayload = {
        guest_name: 'Budi',
        guest_destination: 'Blok A1',
        guest_type: 'TAMU',
      };
      expect(Value.Check(CreateGuestDto, { ...basePayload, guest_identity_type: 'KTP' })).toBe(
        true,
      );
      expect(Value.Check(CreateGuestDto, { ...basePayload, guest_identity_type: 'SIM' })).toBe(
        true,
      );
      expect(Value.Check(CreateGuestDto, { ...basePayload, guest_identity_type: 'PASPOR' })).toBe(
        true,
      );
      expect(Value.Check(CreateGuestDto, { ...basePayload, guest_identity_type: 'LAINNYA' })).toBe(
        true,
      );
      expect(Value.Check(CreateGuestDto, { ...basePayload, guest_identity_type: null })).toBe(true);
      expect(Value.Check(CreateGuestDto, basePayload)).toBe(true);

      // Menolak nilai di luar enum
      expect(Value.Check(CreateGuestDto, { ...basePayload, guest_identity_type: 'Paspor' })).toBe(
        false,
      );
      expect(Value.Check(CreateGuestDto, { ...basePayload, guest_identity_type: 'Lainnya' })).toBe(
        false,
      );
      expect(Value.Check(CreateGuestDto, { ...basePayload, guest_identity_type: 'ktp' })).toBe(
        false,
      );
      expect(Value.Check(CreateGuestDto, { ...basePayload, guest_identity_type: 'KTM' })).toBe(
        false,
      );
      expect(
        Value.Check(CreateGuestDto, { ...basePayload, guest_identity_type: 'Kartu Pelajar' }),
      ).toBe(false);
    });
  });

  describe('UpdateGuestDto', () => {
    it('menolak guest_to_user_email yang bukan format email pada UpdateGuestDto', () => {
      const invalidPayload = {
        guest_to_user_email: 'bukan-email',
      };
      expect(Value.Check(UpdateGuestDto, invalidPayload)).toBe(false);
    });

    it('menerima update parsial dengan email yang valid dan photo URL > 64 karakter', () => {
      const validPayload = {
        guest_to_user_email: 'guard@veranda.com',
        guest_name: 'Nama Baru',
        guest_photo:
          'https://res.cloudinary.com/kawalo/image/upload/v1727083200/guests/profiles/c9b0e1a2-3f4d-5e6a-7b8c-9d0e1f2a3b4c_profile_large.png',
      };
      expect(Value.Check(UpdateGuestDto, validPayload)).toBe(true);
    });

    it('menolak guest_photo pada UpdateGuestDto jika melebihi 2048 karakter', () => {
      const invalidPayload = {
        guest_photo: `https://example.com/${'a'.repeat(2040)}`,
      };
      expect(Value.Check(UpdateGuestDto, invalidPayload)).toBe(false);
    });

    it('menolak nilai null atau string kosong pada field database NOT NULL (guest_name, guest_destination, guest_type)', () => {
      expect(Value.Check(UpdateGuestDto, { guest_name: null })).toBe(false);
      expect(Value.Check(UpdateGuestDto, { guest_destination: null })).toBe(false);
      expect(Value.Check(UpdateGuestDto, { guest_type: null })).toBe(false);
      expect(Value.Check(UpdateGuestDto, { guest_name: '' })).toBe(false);
      expect(Value.Check(UpdateGuestDto, { guest_destination: '' })).toBe(false);
      expect(Value.Check(UpdateGuestDto, { guest_type: '' })).toBe(false);
    });

    it('menerima nilai null pada field database yang NULLABLE (purpose, number, identity, address, photo, email)', () => {
      const validNullablePayload = {
        guest_purpose: null,
        guest_number: null,
        guest_identity: null,
        guest_identity_type: null,
        guest_address: null,
        guest_to_user_email: null,
        guest_photo: null,
      };
      expect(Value.Check(UpdateGuestDto, validNullablePayload)).toBe(true);
    });

    it('menolak panjang string yang melebihi kolom database pada UpdateGuestDto', () => {
      expect(Value.Check(UpdateGuestDto, { guest_name: 'A'.repeat(129) })).toBe(false);
      expect(Value.Check(UpdateGuestDto, { guest_destination: 'A'.repeat(129) })).toBe(false);
      expect(Value.Check(UpdateGuestDto, { guest_type: 'A'.repeat(65) })).toBe(false);
      expect(Value.Check(UpdateGuestDto, { guest_purpose: 'A'.repeat(129) })).toBe(false);
      expect(Value.Check(UpdateGuestDto, { guest_identity: 'A'.repeat(65) })).toBe(false);
      expect(Value.Check(UpdateGuestDto, { guest_identity_type: 'A'.repeat(33) })).toBe(false);
      expect(Value.Check(UpdateGuestDto, { guest_number: 'A'.repeat(33) })).toBe(false);
      expect(Value.Check(UpdateGuestDto, { guest_address: 'A'.repeat(513) })).toBe(false);
    });

    it('hanya menerima guest_identity_type KTP, SIM, PASPOR, LAINNYA, atau null pada UpdateGuestDto', () => {
      expect(Value.Check(UpdateGuestDto, { guest_identity_type: 'KTP' })).toBe(true);
      expect(Value.Check(UpdateGuestDto, { guest_identity_type: 'SIM' })).toBe(true);
      expect(Value.Check(UpdateGuestDto, { guest_identity_type: 'PASPOR' })).toBe(true);
      expect(Value.Check(UpdateGuestDto, { guest_identity_type: 'LAINNYA' })).toBe(true);
      expect(Value.Check(UpdateGuestDto, { guest_identity_type: null })).toBe(true);
      expect(Value.Check(UpdateGuestDto, {})).toBe(true);

      expect(Value.Check(UpdateGuestDto, { guest_identity_type: 'Paspor' })).toBe(false);
      expect(Value.Check(UpdateGuestDto, { guest_identity_type: 'Lainnya' })).toBe(false);
      expect(Value.Check(UpdateGuestDto, { guest_identity_type: 'ktp' })).toBe(false);
      expect(Value.Check(UpdateGuestDto, { guest_identity_type: 'KTM' })).toBe(false);
      expect(Value.Check(UpdateGuestDto, { guest_identity_type: 'Kartu Pelajar' })).toBe(false);
    });
  });

  describe('GetGuestsQueryDto', () => {
    it('menerima query is_active boolean atau boolean string', () => {
      expect(Value.Check(GetGuestsQueryDto, { is_active: true })).toBe(true);
      expect(Value.Check(GetGuestsQueryDto, { is_active: false })).toBe(true);
      expect(Value.Check(GetGuestsQueryDto, { is_active: 'true' })).toBe(true);
      expect(Value.Check(GetGuestsQueryDto, { is_active: 'false' })).toBe(true);
      expect(Value.Check(GetGuestsQueryDto, {})).toBe(true);
    });

    it('menolak is_active selain boolean / boolean string', () => {
      expect(Value.Check(GetGuestsQueryDto, { is_active: 'INVALID' })).toBe(false);
      expect(Value.Check(GetGuestsQueryDto, { is_active: 123 })).toBe(false);
    });
  });
});
