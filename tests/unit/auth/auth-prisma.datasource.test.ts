import { AuthPrismaDataSource } from '@features/auth/data/datasources/auth-prisma.datasource';
import type { PrismaClient } from '@generated/prisma/client';
import { describe, expect, it, vi } from 'vitest';

describe('AuthPrismaDataSource Unit Tests', () => {
  it('seharusnya mengembalikan null jika email falsy atau kosong (input guard)', async () => {
    const mockDb = {
      tkuser: { findUnique: vi.fn() },
      tkusergroup: { findMany: vi.fn() },
      tkgroup: { findMany: vi.fn() },
    } as unknown as PrismaClient;

    const ds = new AuthPrismaDataSource(mockDb);

    expect(await ds.findByEmail('')).toBeNull();
    expect(mockDb.tkuser.findUnique).not.toHaveBeenCalled();
  });

  it('seharusnya mengembalikan null jika id falsy, non-numeric, atau tidak valid (input guard)', async () => {
    const mockDb = {
      tkuser: { findUnique: vi.fn() },
    } as unknown as PrismaClient;

    const ds = new AuthPrismaDataSource(mockDb);

    expect(await ds.findById('')).toBeNull();
    expect(await ds.findById('abc')).toBeNull();
    expect(await ds.findById('-5')).toBeNull();
    expect(mockDb.tkuser.findUnique).not.toHaveBeenCalled();
  });

  it('seharusnya memetakan user dan grup dengan 100% pra-sanitasi, explicit select, dan safe map lookup via findByEmail', async () => {
    const mockDb = {
      tkuser: {
        findUnique: vi.fn().mockResolvedValue({
          UserId: 101,
          UserEmail: 'satpam.budi@kawalo.id',
          UserName: 'Budi Security',
          Password: 'plainPasswordOrHash',
          UserProfilePic: 'N/A', // Dirty string, harus disanitasi ke null
        }),
      },
      tkusergroup: {
        findMany: vi.fn().mockResolvedValue([
          {
            GroupUID: 'GRP-01',
            Email: 'satpam.budi@kawalo.id',
            AllowAdmin: 'YES',
            AllowPatrol: 'yes', // Mixed case, harus diparsing ke true
            AllowMonPatrol: 'NO',
            Category: 'SUPERVISOR',
            Status: 'JOINED',
          },
          {
            GroupUID: 'GRP-02',
            Email: 'satpam.budi@kawalo.id',
            AllowAdmin: 'NO',
            AllowPatrol: 'NO',
            AllowMonPatrol: 'NO',
            Category: 'N/A', // Dirty category, harus null murni (zero hallucination)
            Status: 'PENDING',
          },
        ]),
      },
      tkgroup: {
        findMany: vi.fn().mockResolvedValue([
          {
            GroupUID: 'GRP-01',
            GroupName: 'Grup Kawalo Utama',
            GroupOwner: 'PT Kawalo Solusi',
            GroupLogo: 'N/A', // Dirty string, harus null
            Status: 'CLIENT',
            Features: 'PATROL;GUEST;MONITOR;N/A;', // Dirty feature flag 'N/A' harus difilter
          },
          {
            GroupUID: 'GRP-02',
            GroupName: 'Grup Cadangan',
            GroupOwner: null,
            GroupLogo: null,
            Status: 'TRIAL',
            Features: null,
          },
        ]),
      },
    } as unknown as PrismaClient;

    const ds = new AuthPrismaDataSource(mockDb);
    const user = await ds.findByEmail('satpam.budi@kawalo.id'); // Pre-sanitized input dari boundary request

    expect(user).toBeDefined();
    if (!user) throw new Error('User tidak ditemukan');

    // 1. Verifikasi entity user
    expect(user.id).toBe('101');
    expect(user.email).toBe('satpam.budi@kawalo.id'); // Otomatis lowercase
    expect(user.name).toBe('Budi Security');
    expect(user.avatarUrl).toBeNull(); // 'N/A' dinormalisasi ke null

    // 2. Verifikasi activeGroup (default JOINED)
    expect(user.activeGroup).toBeDefined();
    expect(user.activeGroup?.groupUid).toBe('GRP-01');
    expect(user.activeGroup?.groupName).toBe('Grup Kawalo Utama');
    expect(user.activeGroup?.groupOwner).toBe('PT Kawalo Solusi');
    expect(user.activeGroup?.groupLogo).toBeNull();
    expect(user.activeGroup?.category).toBe('SUPERVISOR');
    expect(user.role).toBe('SUPERVISOR');
    expect(user.allowAdmin).toBe(true);
    expect(user.allowPatrol).toBe(true);
    expect(user.allowMonPatrol).toBe(false);
    expect(user.activeGroup?.features).toEqual(['PATROL', 'GUEST', 'MONITOR']); // 'N/A' bersih

    // 3. Verifikasi grup kedua dengan category kotor
    expect(user.groups).toHaveLength(2);
    expect(user.groups[1]?.groupUid).toBe('GRP-02');
    expect(user.groups[1]?.category).toBeNull(); // 'N/A' dinormalisasi ke null murni

    // 4. Verifikasi explicit select pada tkuser
    expect(mockDb.tkuser.findUnique).toHaveBeenCalledWith(
      expect.objectContaining({
        where: { UserEmail: 'satpam.budi@kawalo.id' },
        select: {
          UserId: true,
          UserEmail: true,
          UserName: true,
          Password: true,
          UserProfilePic: true,
        },
      }),
    );

    // 5. Verifikasi explicit select pada tkusergroup (hanya 6 kolom, tanpa Email)
    expect(mockDb.tkusergroup.findMany).toHaveBeenCalledWith(
      expect.objectContaining({
        where: { Email: 'satpam.budi@kawalo.id' },
        select: {
          GroupUID: true,
          AllowAdmin: true,
          AllowPatrol: true,
          AllowMonPatrol: true,
          Category: true,
          Status: true,
        },
      }),
    );

    // 6. Verifikasi explicit select pada tkgroup (hanya 6 kolom yang dipetakan)
    expect(mockDb.tkgroup.findMany).toHaveBeenCalledWith(
      expect.objectContaining({
        where: { GroupUID: { in: ['GRP-01', 'GRP-02'] } },
        select: {
          GroupUID: true,
          GroupName: true,
          GroupOwner: true,
          GroupLogo: true,
          Status: true,
          Features: true,
        },
      }),
    );
  });

  it('seharusnya menghormati requestedGroupUid jika diberikan', async () => {
    const mockDb = {
      tkuser: {
        findUnique: vi.fn().mockResolvedValue({
          UserId: 102,
          UserEmail: 'user2@kawalo.id',
          UserName: 'User Dua',
          Password: 'secretPassword',
          UserProfilePic: 'https://cdn.kawalo.id/user2.png',
        }),
      },
      tkusergroup: {
        findMany: vi.fn().mockResolvedValue([
          {
            GroupUID: 'GRP-A',
            Email: 'user2@kawalo.id',
            AllowAdmin: 'NO',
            AllowPatrol: 'NO',
            AllowMonPatrol: 'NO',
            Category: 'STAFF',
            Status: 'JOINED',
          },
          {
            GroupUID: 'GRP-B',
            Email: 'user2@kawalo.id',
            AllowAdmin: 'YES',
            AllowPatrol: 'YES',
            AllowMonPatrol: 'YES',
            Category: 'MANAGER',
            Status: 'JOINED',
          },
        ]),
      },
      tkgroup: {
        findMany: vi.fn().mockResolvedValue([
          {
            GroupUID: 'GRP-A',
            GroupName: 'Grup A',
            GroupOwner: null,
            GroupLogo: null,
            Status: 'ACTIVE',
            Features: '',
          },
          {
            GroupUID: 'GRP-B',
            GroupName: 'Grup B',
            GroupOwner: null,
            GroupLogo: null,
            Status: 'ACTIVE',
            Features: '',
          },
        ]),
      },
    } as unknown as PrismaClient;

    const ds = new AuthPrismaDataSource(mockDb);
    const user = await ds.findById('102', 'GRP-B');

    expect(user).toBeDefined();
    if (!user) throw new Error('User tidak ditemukan');

    expect(user.activeGroup?.groupUid).toBe('GRP-B');
    expect(user.role).toBe('MANAGER');
    expect(user.allowAdmin).toBe(true);
  });

  it('seharusnya mengembalikan null jika data mentah tkuser korup demi zero data hallucination & zero data corruption', async () => {
    const mockDb = {
      tkuser: {
        findUnique: vi.fn(),
      },
      tkusergroup: { findMany: vi.fn() },
      tkgroup: { findMany: vi.fn() },
    } as unknown as PrismaClient;

    const ds = new AuthPrismaDataSource(mockDb);

    // Kasus 1: UserEmail bernilai 'N/A' (dirty)
    (mockDb.tkuser.findUnique as any).mockResolvedValueOnce({
      UserId: 201,
      UserEmail: 'N/A',
      UserName: 'User Korup',
      Password: 'hash',
      UserProfilePic: null,
    } as any);
    expect(await ds.findByEmail('corrupt1@kawalo.id')).toBeNull();

    // Kasus 2: UserName kosong atau whitespace murni
    (mockDb.tkuser.findUnique as any).mockResolvedValueOnce({
      UserId: 202,
      UserEmail: 'corrupt2@kawalo.id',
      UserName: '   ',
      Password: 'hash',
      UserProfilePic: null,
    } as any);
    expect(await ds.findById('202')).toBeNull();

    // Kasus 3: Password kosong atau null
    (mockDb.tkuser.findUnique as any).mockResolvedValueOnce({
      UserId: 203,
      UserEmail: 'corrupt3@kawalo.id',
      UserName: 'No Password User',
      Password: '   ',
      UserProfilePic: null,
    } as any);
    expect(await ds.findById('203')).toBeNull();

    // Kasus 4: UserId korup / bukan integer
    (mockDb.tkuser.findUnique as any).mockResolvedValueOnce({
      UserId: 'not-a-number' as any,
      UserEmail: 'corrupt4@kawalo.id',
      UserName: 'Invalid ID User',
      Password: 'validPassword',
      UserProfilePic: null,
    } as any);
    expect(await ds.findById('204')).toBeNull();

    // Pastikan tidak ada query relasi grup yang dieksekusi jika data user korup
    expect(mockDb.tkusergroup.findMany).not.toHaveBeenCalled();
  });

  it('seharusnya tidak melakukan fallback ke grup JOINED jika requestedGroupUid tidak cocok dengan keanggotaan user', async () => {
    const mockDb = {
      tkuser: {
        findUnique: vi.fn().mockResolvedValue({
          UserId: 301,
          UserEmail: 'user301@kawalo.id',
          UserName: 'User Tiga Ratus Satu',
          Password: 'secretPassword',
          UserProfilePic: null,
        }),
      },
      tkusergroup: {
        findMany: vi.fn().mockResolvedValue([
          {
            GroupUID: 'GRP-REAL',
            Email: 'user301@kawalo.id',
            AllowAdmin: 'NO',
            AllowPatrol: 'YES',
            AllowMonPatrol: 'NO',
            Category: 'GUARD',
            Status: 'JOINED',
          },
        ]),
      },
      tkgroup: {
        findMany: vi.fn().mockResolvedValue([
          {
            GroupUID: 'GRP-REAL',
            GroupName: 'Grup Nyata',
            GroupOwner: null,
            GroupLogo: null,
            Status: 'ACTIVE',
            Features: '',
          },
        ]),
      },
    } as unknown as PrismaClient;

    const ds = new AuthPrismaDataSource(mockDb);

    // Saat requestedGroupUid tidak ada di user groups, activeGroup HARUS null (Zero Silent Fallback)
    const user = await ds.findById('301', 'GRP-NONEXISTENT');

    expect(user).toBeDefined();
    expect(user?.activeGroup).toBeNull();
    expect(user?.groups).toHaveLength(1);
    expect(user?.groups[0]?.groupUid).toBe('GRP-REAL');
  });
});
