import { GuestEntity } from '@features/guest/domain/entities/guest.entity';
import type { MasterGuestTypeItem } from '@features/guest/domain/entities/master-data.entity';
import { GuestResponseMapper } from '@features/guest/presentation/mappers/guest-response.mapper';
import { describe, expect, it } from 'vitest';

describe('GuestResponseMapper Unit Tests', () => {
  it('berhasil memetakan GuestEntity ke GuestItemDtoType', () => {
    const entity = new GuestEntity({
      id: '1',
      guestUid: 'GST-001',
      groupUid: 'GRP-001',
      groupName: 'Veranda Group',
      guestName: 'Budi Santoso',
      guestNumber: 'B 1234 CD',
      guestIdentity: '3201234567890001',
      guestIdentityType: 'KTP',
      guestPhoto: 'photo.jpg',
      guestDt: '2026-09-17T10:00:00.000Z',
      guestAckDt: '2026-09-17T10:05:00.000Z',
      guestOutDt: '2026-09-17T12:00:00.000Z',
      guestPurpose: 'Kunjungan',
      guestDestination: 'Blok A1',
      guestToUserEmail: 'resident@veranda.com',
      guestToUserName: 'Resident One',
      guestCreatorEmail: 'guard@veranda.com',
      guestCreatorName: 'Guard User',
      guestCategory: 'Umum',
      qrUid: 'QR-001',
      guestAddress: 'Jakarta',
      guestType: 'TAMU',
      guestCheckOutByEmail: 'guard@veranda.com',
      guestCheckOutByName: 'Guard User',
    });

    const response = GuestResponseMapper.toResponse(entity);

    expect(response).toEqual({
      id: '1',
      group_name: 'Veranda Group',
      guest_name: 'Budi Santoso',
      guest_number: 'B 1234 CD',
      guest_identity: '3201234567890001',
      guest_identity_type: 'KTP',
      guest_photo: 'photo.jpg',
      guest_dt: '2026-09-17T10:00:00.000Z',
      guest_ack_dt: '2026-09-17T10:05:00.000Z',
      guest_out_dt: '2026-09-17T12:00:00.000Z',
      guest_purpose: 'Kunjungan',
      guest_destination: 'Blok A1',
      guest_to_user_email: 'resident@veranda.com',
      guest_to_user_name: 'Resident One',
      guest_creator_email: 'guard@veranda.com',
      guest_creator_name: 'Guard User',
      guest_category: 'Umum',
      qr_uid: 'QR-001',
      guest_address: 'Jakarta',
      guest_type: 'TAMU',
      guest_checkout_by_email: 'guard@veranda.com',
      guest_checkout_by_name: 'Guard User',
    });
  });

  it('berhasil memetakan daftar GuestEntity via toResponseList', () => {
    const entity = new GuestEntity({
      id: '1',
      groupUid: 'GRP-001',
      guestName: 'Tamu 1',
      guestDestination: 'Blok A',
      guestType: 'TAMU',
    });

    const list = GuestResponseMapper.toResponseList([entity]);
    expect(list).toHaveLength(1);
    expect(list[0]?.id).toBe('1');
    expect(list[0]?.guest_name).toBe('Tamu 1');
  });

  it('berhasil memetakan MasterGuestTypeItem via toMasterGuestType dan toMasterGuestTypeList', () => {
    const item: MasterGuestTypeItem = {
      id: '10',
      uid: 'TYPE-01',
      name: 'TAMU VIP',
    };

    const mapped = GuestResponseMapper.toMasterGuestType(item);
    expect(mapped).toEqual({
      id: '10',
      uid: 'TYPE-01',
      name: 'TAMU VIP',
    });

    const list = GuestResponseMapper.toMasterGuestTypeList([item]);
    expect(list).toHaveLength(1);
    expect(list[0]).toEqual({
      id: '10',
      uid: 'TYPE-01',
      name: 'TAMU VIP',
    });
  });
});
