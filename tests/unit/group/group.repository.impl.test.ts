import type { GroupPrismaDataSource } from '@features/group/data/datasources/group-prisma.datasource';
import { GroupRepositoryImpl } from '@features/group/data/repositories/group.repository.impl';
import { beforeEach, describe, expect, it, vi } from 'vitest';

describe('GroupRepositoryImpl Unit Tests', () => {
  let mockPrismaSource: {
    findActiveMemberEmailsByGroup: ReturnType<typeof vi.fn>;
    findUsersByEmails: ReturnType<typeof vi.fn>;
  };
  let repository: GroupRepositoryImpl;

  beforeEach(() => {
    mockPrismaSource = {
      findActiveMemberEmailsByGroup: vi.fn(),
      findUsersByEmails: vi.fn(),
    };

    repository = new GroupRepositoryImpl(mockPrismaSource as unknown as GroupPrismaDataSource);
  });

  describe('getMembers', () => {
    it('mengembalikan array kosong jika groupUid tidak valid', async () => {
      const result = await repository.getMembers('');
      expect(result).toEqual([]);
      expect(mockPrismaSource.findActiveMemberEmailsByGroup).not.toHaveBeenCalled();
    });

    it('mengembalikan array kosong jika tidak ada anggota grup aktif (JOINED)', async () => {
      mockPrismaSource.findActiveMemberEmailsByGroup.mockResolvedValue([]);

      const result = await repository.getMembers('GRP-001');

      expect(result).toEqual([]);
      expect(mockPrismaSource.findActiveMemberEmailsByGroup).toHaveBeenCalledWith('GRP-001');
      expect(mockPrismaSource.findUsersByEmails).not.toHaveBeenCalled();
    });

    it('mengorkestrasikan pengambilan anggota grup, profil, foto, fallback, dan sorting alfabetis', async () => {
      mockPrismaSource.findActiveMemberEmailsByGroup.mockResolvedValue([
        'candra@veranda.com',
        'ahmad@veranda.com',
        'budi@veranda.com',
      ]);

      mockPrismaSource.findUsersByEmails.mockResolvedValue([
        {
          userEmail: 'ahmad@veranda.com',
          userName: 'Zulkifli Ahmad',
          userProfilePic: 'https://ahmad.png',
        },
        { userEmail: 'budi@veranda.com', userName: 'Budi Santoso', userProfilePic: null },
      ]);

      const result = await repository.getMembers('GRP-001');

      expect(mockPrismaSource.findActiveMemberEmailsByGroup).toHaveBeenCalledWith('GRP-001');
      expect(mockPrismaSource.findUsersByEmails).toHaveBeenCalledWith([
        'candra@veranda.com',
        'ahmad@veranda.com',
        'budi@veranda.com',
      ]);

      expect(result).toEqual([
        { email: 'budi@veranda.com', name: 'Budi Santoso', profilePic: null },
        { email: 'candra@veranda.com', name: null, profilePic: null },
        { email: 'ahmad@veranda.com', name: 'Zulkifli Ahmad', profilePic: 'https://ahmad.png' },
      ]);
    });
  });
});
