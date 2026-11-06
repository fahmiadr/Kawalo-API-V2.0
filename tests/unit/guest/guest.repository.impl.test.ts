import type { GuestPrismaDataSource } from '@features/guest/data/datasources/guest-prisma.datasource';
import { GuestRepositoryImpl } from '@features/guest/data/repositories/guest.repository.impl';
import { GuestEntity } from '@features/guest/domain/entities/guest.entity';
import type {
  CreateGuestParams,
  UpdateGuestParams,
} from '@features/guest/domain/repositories/guest.repository';
import { beforeEach, describe, expect, it, vi } from 'vitest';

describe('GuestRepositoryImpl Unit Tests', () => {
  let mockPrismaSource: {
    getGuests: ReturnType<typeof vi.fn>;
    getGuestById: ReturnType<typeof vi.fn>;
    getGuestsByIds: ReturnType<typeof vi.fn>;
    createGuest: ReturnType<typeof vi.fn>;
    updateGuest: ReturnType<typeof vi.fn>;
    deleteManyGuests: ReturnType<typeof vi.fn>;
    findActiveMemberEmailsByGroup: ReturnType<typeof vi.fn>;
    findUsersByEmails: ReturnType<typeof vi.fn>;
    isGroupMemberJoined: ReturnType<typeof vi.fn>;
    resolveCanonicalGuestType: ReturnType<typeof vi.fn>;
    getMasterGuestTypes: ReturnType<typeof vi.fn>;
  };
  let repository: GuestRepositoryImpl;

  beforeEach(() => {
    mockPrismaSource = {
      getGuests: vi.fn(),
      getGuestById: vi.fn(),
      getGuestsByIds: vi.fn(),
      createGuest: vi.fn(),
      updateGuest: vi.fn(),
      deleteManyGuests: vi.fn(),
      findActiveMemberEmailsByGroup: vi.fn(),
      findUsersByEmails: vi.fn(),
      isGroupMemberJoined: vi.fn(),
      resolveCanonicalGuestType: vi.fn(),
      getMasterGuestTypes: vi.fn(),
    };

    repository = new GuestRepositoryImpl(mockPrismaSource as unknown as GuestPrismaDataSource);
  });

  describe('isValidHost', () => {
    it('mengembalikan false jika email atau groupUid kosong', async () => {
      expect(await repository.isValidHost('', 'GRP-001')).toBe(false);
      expect(await repository.isValidHost('host@veranda.com', '')).toBe(false);
      expect(mockPrismaSource.isGroupMemberJoined).not.toHaveBeenCalled();
    });

    it('memanggil isGroupMemberJoined dan mengembalikan hasilnya', async () => {
      mockPrismaSource.isGroupMemberJoined.mockResolvedValue(true);

      const isValid = await repository.isValidHost('host@veranda.com', 'GRP-001');

      expect(isValid).toBe(true);
      expect(mockPrismaSource.isGroupMemberJoined).toHaveBeenCalledWith(
        'host@veranda.com',
        'GRP-001',
      );
    });
  });

  describe('isValidGuestType', () => {
    it('mengembalikan false jika guestType atau groupUid kosong', async () => {
      expect(await repository.isValidGuestType('', 'GRP-001')).toBe(false);
      expect(await repository.isValidGuestType('TAMU', '')).toBe(false);
      expect(mockPrismaSource.resolveCanonicalGuestType).not.toHaveBeenCalled();
    });

    it('mengembalikan true jika tipe tamu kanonikal ditemukan', async () => {
      mockPrismaSource.resolveCanonicalGuestType.mockResolvedValue('TAMU UMUM');

      const isValid = await repository.isValidGuestType('tamu umum', 'GRP-001');

      expect(isValid).toBe(true);
      expect(mockPrismaSource.resolveCanonicalGuestType).toHaveBeenCalledWith(
        'tamu umum',
        'GRP-001',
      );
    });

    it('mengembalikan false jika tipe tamu kanonikal bernilai null', async () => {
      mockPrismaSource.resolveCanonicalGuestType.mockResolvedValue(null);

      const isValid = await repository.isValidGuestType('UNKNOWN', 'GRP-001');

      expect(isValid).toBe(false);
    });
  });

  describe('resolveCanonicalGuestType', () => {
    it('mengembalikan null jika input kosong', async () => {
      expect(await repository.resolveCanonicalGuestType('', 'GRP-001')).toBeNull();
      expect(await repository.resolveCanonicalGuestType('TAMU', '')).toBeNull();
      expect(mockPrismaSource.resolveCanonicalGuestType).not.toHaveBeenCalled();
    });

    it('mendelegasikan ke prismaSource jika input valid', async () => {
      mockPrismaSource.resolveCanonicalGuestType.mockResolvedValue('TAMU RESMI');

      const result = await repository.resolveCanonicalGuestType('TAMU', 'GRP-001');

      expect(result).toBe('TAMU RESMI');
      expect(mockPrismaSource.resolveCanonicalGuestType).toHaveBeenCalledWith('TAMU', 'GRP-001');
    });
  });

  describe('getMasterGuestTypes', () => {
    it('mengembalikan array kosong jika groupUid kosong', async () => {
      expect(await repository.getMasterGuestTypes('')).toEqual([]);
      expect(mockPrismaSource.getMasterGuestTypes).not.toHaveBeenCalled();
    });

    it('mendelegasikan ke prismaSource jika groupUid valid', async () => {
      mockPrismaSource.getMasterGuestTypes.mockResolvedValue([
        { id: '1', uid: 'UID-1', name: 'KURIR' },
      ]);

      const result = await repository.getMasterGuestTypes('GRP-001');

      expect(result).toEqual([{ id: '1', uid: 'UID-1', name: 'KURIR' }]);
      expect(mockPrismaSource.getMasterGuestTypes).toHaveBeenCalledWith('GRP-001');
    });
  });

  describe('CRUD delegation', () => {
    const dummyEntity = new GuestEntity({
      id: '1',
      groupUid: 'GRP-001',
      guestName: 'Guest 1',
      guestDestination: 'A1',
    });

    it('getGuests mendelegasikan ke prismaSource', async () => {
      mockPrismaSource.getGuests.mockResolvedValue([dummyEntity]);

      const result = await repository.getGuests('GRP-001', { limit: 10 });

      expect(result).toEqual([dummyEntity]);
      expect(mockPrismaSource.getGuests).toHaveBeenCalledWith('GRP-001', { limit: 10 });
    });

    it('getGuestById mendelegasikan ke prismaSource', async () => {
      mockPrismaSource.getGuestById.mockResolvedValue(dummyEntity);

      const result = await repository.getGuestById('1', 'GRP-001');

      expect(result).toBe(dummyEntity);
      expect(mockPrismaSource.getGuestById).toHaveBeenCalledWith('1', 'GRP-001');
    });

    it('getGuestsByIds mendelegasikan ke prismaSource', async () => {
      mockPrismaSource.getGuestsByIds.mockResolvedValue([dummyEntity]);

      const result = await repository.getGuestsByIds(['1'], 'GRP-001');

      expect(result).toEqual([dummyEntity]);
      expect(mockPrismaSource.getGuestsByIds).toHaveBeenCalledWith(['1'], 'GRP-001');
    });

    it('createGuest mendelegasikan ke prismaSource', async () => {
      mockPrismaSource.createGuest.mockResolvedValue(dummyEntity);
      const params = {
        context: { groupUid: 'GRP-001' },
      } as unknown as CreateGuestParams;

      const result = await repository.createGuest(params);

      expect(result).toBe(dummyEntity);
      expect(mockPrismaSource.createGuest).toHaveBeenCalledWith(params);
    });

    it('updateGuest mendelegasikan ke prismaSource', async () => {
      mockPrismaSource.updateGuest.mockResolvedValue(dummyEntity);
      const params = {
        context: { id: '1', groupUid: 'GRP-001' },
      } as unknown as UpdateGuestParams;

      const result = await repository.updateGuest(params);

      expect(result).toBe(dummyEntity);
      expect(mockPrismaSource.updateGuest).toHaveBeenCalledWith(params);
    });

    it('deleteManyGuests mendelegasikan ke prismaSource', async () => {
      mockPrismaSource.deleteManyGuests.mockResolvedValue(3);

      const result = await repository.deleteManyGuests(['1', '2', '3'], 'GRP-001');

      expect(result).toBe(3);
      expect(mockPrismaSource.deleteManyGuests).toHaveBeenCalledWith(['1', '2', '3'], 'GRP-001');
    });
  });
});
