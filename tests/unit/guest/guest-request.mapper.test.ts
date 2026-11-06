import { AppConstants } from '@core/constants/app.constants';
import type {
  CreateGuestDtoType,
  UpdateGuestDtoType,
} from '@features/guest/presentation/dtos/guest-request.dto';
import { GuestRequestMapper } from '@features/guest/presentation/mappers/guest-request.mapper';
import { describe, expect, it } from 'vitest';

describe('GuestRequestMapper Unit Tests', () => {
  const mockCurrentUser = {
    sub: 'USR-001',
    email: 'admin@veranda.com',
    name: 'Admin User',
    role: 'ADMIN',
    group_uid: 'GRP-001',
    session_ver: 1,
  };

  describe('toGetGuestsInput', () => {
    it('berhasil membersihkan input query dan pagination ke GetGuestsInput yang valid', () => {
      const query = {
        search: '  Budi Santoso  ',
        limit: 15,
        offset: 30,
        start_date: '2026-09-01T00:00:00.000Z',
        end_date: '2026-09-10T23:59:59.000Z',
      };

      const input = GuestRequestMapper.toGetGuestsInput(query, mockCurrentUser);

      expect(input.search).toBe('Budi Santoso');
      expect(input.limit).toBe(15);
      expect(input.offset).toBe(30);
      expect(input.startDate).toBe('2026-09-01T00:00:00.000Z');
      expect(input.endDate).toBe('2026-09-10T23:59:59.000Z');
      expect(input.currentUser).toEqual(mockCurrentUser);
    });

    it('mengonversi string kosong, whitespace, null, atau N/A menjadi undefined', () => {
      const query = {
        search: '   ',
        start_date: 'null',
        end_date: 'N/A',
      };

      const input = GuestRequestMapper.toGetGuestsInput(query, mockCurrentUser);

      expect(input.search).toBeUndefined();
      expect(input.startDate).toBeUndefined();
      expect(input.endDate).toBeUndefined();
    });

    it('menyelesaikan paginasi default jika limit dan offset tidak diisi', () => {
      const query = {};
      const input = GuestRequestMapper.toGetGuestsInput(query, mockCurrentUser);

      expect(input.limit).toBe(AppConstants.DEFAULT_PAGE_SIZE);
      expect(input.offset).toBe(0);
    });

    it('menangani limit 0 sebagai query tanpa batas (take: undefined)', () => {
      const query = { limit: 0 };
      const input = GuestRequestMapper.toGetGuestsInput(query, mockCurrentUser);

      expect(input.limit).toBeUndefined();
      expect(input.offset).toBeUndefined();
    });

    it('memetakan filter is_active ke boolean (true, false, string true/false/1/0)', () => {
      expect(
        GuestRequestMapper.toGetGuestsInput({ is_active: true }, mockCurrentUser).isActive,
      ).toBe(true);
      expect(
        GuestRequestMapper.toGetGuestsInput({ is_active: false }, mockCurrentUser).isActive,
      ).toBe(false);
      expect(
        GuestRequestMapper.toGetGuestsInput({ is_active: 'true' as any }, mockCurrentUser).isActive,
      ).toBe(true);
      expect(
        GuestRequestMapper.toGetGuestsInput({ is_active: 'false' as any }, mockCurrentUser)
          .isActive,
      ).toBe(false);
      expect(
        GuestRequestMapper.toGetGuestsInput({ is_active: 'invalid' as any }, mockCurrentUser)
          .isActive,
      ).toBeUndefined();
    });
  });

  describe('toCreateGuestInput', () => {
    it('berhasil memetakan dan membersihkan field CreateGuestDto', () => {
      const body: CreateGuestDtoType = {
        guest_name: '  Budi Santoso  ',
        guest_destination: '  Blok A1 No. 5  ',
        guest_purpose: '  Kunjungan  ',
        guest_number: '  B 1234 CD  ',
        guest_identity: '3201234567890001',
        guest_identity_type: 'KTP',
        guest_type: '  TAMU  ',
        guest_address: '  Jakarta  ',
        guest_to_user_email: '  Resident@VERANDA.COM  ',
        guest_photo: 'foto.jpg',
        guest_dt: '2026-09-17T10:00:00.000Z',
      };

      const input = GuestRequestMapper.toCreateGuestInput(body, mockCurrentUser);

      expect(input.currentUser).toBe(mockCurrentUser);
      expect(input.data.guestName).toBe('Budi Santoso');
      expect(input.data.guestDestination).toBe('Blok A1 No. 5');
      expect(input.data.guestPurpose).toBe('Kunjungan');
      expect(input.data.guestNumber).toBe('B 1234 CD');
      expect(input.data.guestIdentity).toBe('3201234567890001');
      expect(input.data.guestIdentityType).toBe('KTP');
      expect(input.data.guestType).toBe('TAMU');
      expect(input.data.guestAddress).toBe('Jakarta');
      expect(input.data.guestToUserEmail).toBe('resident@veranda.com'); // otomatis normalized lowercase
      expect(input.data.guestPhoto).toBe('foto.jpg');
      expect(input.data.guestDt).toBe('2026-09-17T10:00:00.000Z');
      expect((input.data as Record<string, unknown>).guestCategory).toBeUndefined();
    });

    it('membersihkan data kotor N/A, whitespace, dan null menjadi undefined pada field opsional', () => {
      const body = {
        guest_name: 'Budi Santoso',
        guest_destination: 'Blok A1',
        guest_type: 'TAMU',
        guest_purpose: '  N/A  ',
        guest_number: 'null',
        guest_address: '   ',
        guest_to_user_email: '   ',
        guest_dt: '1970-01-01T00:00:00.000Z', // epoch date dibersihkan
      };

      const input = GuestRequestMapper.toCreateGuestInput(body, mockCurrentUser);

      expect(input.data.guestPurpose).toBeUndefined();
      expect(input.data.guestNumber).toBeUndefined();
      expect(input.data.guestAddress).toBeUndefined();
      expect(input.data.guestToUserEmail).toBeUndefined();
      expect(input.data.guestDt).toBeUndefined();
    });

    it('mengonversi guest_name, guest_destination, dan id yang kotor menjadi undefined tanpa fallback unsanitized', () => {
      const body = {
        guest_name: '   ',
        guest_destination: 'N/A',
        guest_type: 'null',
      };

      const input = GuestRequestMapper.toCreateGuestInput(body, mockCurrentUser);

      expect(input.data.guestName).toBeUndefined();
      expect(input.data.guestDestination).toBeUndefined();
      expect(input.data.guestType).toBeUndefined();

      const updateInput = GuestRequestMapper.toUpdateGuestInput({ id: '   ' }, {}, mockCurrentUser);
      expect(updateInput.id).toBeUndefined();

      const ackInput = GuestRequestMapper.toAcknowledgeGuestInput(
        { id: '  N/A  ' },
        {},
        mockCurrentUser,
      );
      expect(ackInput.id).toBeUndefined();

      const checkoutInput = GuestRequestMapper.toCheckoutGuestInput(
        { id: 'null' },
        {},
        mockCurrentUser,
      );
      expect(checkoutInput.id).toBeUndefined();

      const deleteInput = GuestRequestMapper.toDeleteGuestInput({ id: '   ' }, mockCurrentUser);
      expect(deleteInput.id).toBeUndefined();
    });
  });

  describe('toUpdateGuestInput', () => {
    it('berhasil memetakan update payload sesuai spesifikasi cleanQueryString dan cleanQueryEmail', () => {
      const body: UpdateGuestDtoType = {
        guest_name: '  Nama Baru  ',
        guest_destination: '  Blok B2  ',
        guest_purpose: '  Pengiriman  ',
        guest_number: '  D 5678 EF  ',
        guest_identity: '3201234567890002',
        guest_identity_type: 'SIM',
        guest_type: 'KURIR',
        guest_address: 'Bandung',
        guest_to_user_email: 'HOST@Veranda.COM',
        guest_photo: 'new-photo.jpg',
      };

      const input = GuestRequestMapper.toUpdateGuestInput({ id: '  99  ' }, body, mockCurrentUser);

      expect(input.id).toBe('99');
      expect(input.data.guestName).toBe('Nama Baru');
      expect(input.data.guestDestination).toBe('Blok B2');
      expect(input.data.guestPurpose).toBe('Pengiriman');
      expect(input.data.guestNumber).toBe('D 5678 EF');
      expect(input.data.guestIdentity).toBe('3201234567890002');
      expect(input.data.guestIdentityType).toBe('SIM');
      expect(input.data.guestType).toBe('KURIR');
      expect(input.data.guestAddress).toBe('Bandung');
      expect(input.data.guestToUserEmail).toBe('host@veranda.com');
      expect(input.data.guestPhoto).toBe('new-photo.jpg');
    });

    it('mengubah input kotor N/A, null, dan string kosong pada update data menjadi null (explicit clear)', () => {
      const body = {
        guest_purpose: 'N/A',
        guest_number: 'null',
        guest_address: '   ',
        guest_to_user_email: '  N/A  ',
        guest_photo: null,
      };

      const input = GuestRequestMapper.toUpdateGuestInput({ id: '100' }, body, mockCurrentUser);

      expect(input.data.guestPurpose).toBeNull();
      expect(input.data.guestNumber).toBeNull();
      expect(input.data.guestAddress).toBeNull();
      expect(input.data.guestToUserEmail).toBeNull();
      expect(input.data.guestPhoto).toBeNull();
      expect(input.data.guestName).toBeUndefined();
      expect(input.data.guestDestination).toBeUndefined();
    });
  });


  describe('toAcknowledgeGuestInput', () => {
    it('memetakan acknowledge input dengan sanitasi id dan qr_uid opsional', () => {
      const body = {
        qr_uid: '  QR-ACK-001  ',
      };

      const input = GuestRequestMapper.toAcknowledgeGuestInput(
        { id: '  50  ' },
        body,
        mockCurrentUser,
      );

      expect(input.id).toBe('50');
      expect(input.data?.qrUid).toBe('QR-ACK-001');
      expect(input.currentUser).toEqual(mockCurrentUser);
    });

    it('membersihkan qr_uid kotor menjadi undefined', () => {
      const body = {
        qr_uid: '   ',
      };

      const input = GuestRequestMapper.toAcknowledgeGuestInput({ id: '50' }, body, mockCurrentUser);

      expect(input.id).toBe('50');
      expect(input.data?.qrUid).toBeUndefined();
    });
  });

  describe('toCheckoutGuestInput', () => {
    it('memetakan checkout input dengan sanitasi id dan qr_uid opsional', () => {
      const body = {
        qr_uid: '  QR-OUT-001  ',
      };

      const input = GuestRequestMapper.toCheckoutGuestInput(
        { id: '  50  ' },
        body,
        mockCurrentUser,
      );

      expect(input.id).toBe('50');
      expect(input.data?.qrUid).toBe('QR-OUT-001');
      expect(input.currentUser).toEqual(mockCurrentUser);
    });

    it('membersihkan qr_uid kotor menjadi undefined', () => {
      const body = {
        qr_uid: 'null',
      };

      const input = GuestRequestMapper.toCheckoutGuestInput({ id: '50' }, body, mockCurrentUser);

      expect(input.id).toBe('50');
      expect(input.data?.qrUid).toBeUndefined();
    });
  });

  describe('toGetGuestByIdInput', () => {
    it('memetakan params id dan membersihkan whitespace', () => {
      const input = GuestRequestMapper.toGetGuestByIdInput({ id: '  QR-12345  ' }, mockCurrentUser);

      expect(input.id).toBe('QR-12345');
      expect(input.currentUser).toEqual(mockCurrentUser);
    });

    it('mengembalikan undefined jika id null/undefined setelah sanitasi', () => {
      const input = GuestRequestMapper.toGetGuestByIdInput({ id: '   ' }, mockCurrentUser);

      expect(input.id).toBeUndefined();
      expect(input.currentUser).toEqual(mockCurrentUser);
    });
  });

  describe('toDeleteGuestInput', () => {
    it('memetakan delete input dengan sanitasi id', () => {
      const input = GuestRequestMapper.toDeleteGuestInput({ id: '  75  ' }, mockCurrentUser);

      expect(input.id).toBe('75');
      expect(input.currentUser).toEqual(mockCurrentUser);
    });
  });

  describe('toGetMasterGuestTypesInput', () => {
    it('meneruskan currentUser secara utuh ke input usecase master data', () => {
      const typesInput = GuestRequestMapper.toGetMasterGuestTypesInput(mockCurrentUser);
      expect(typesInput.currentUser).toEqual(mockCurrentUser);
    });
  });
});
