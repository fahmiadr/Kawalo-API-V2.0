import { GuestPrismaDataSource } from '@features/guest/data/datasources/guest-prisma.datasource';
import { GuestResponseMapper } from '@features/guest/presentation/mappers/guest-response.mapper';
import type { PrismaClient } from '@generated/prisma/client';
import { describe, expect, it, vi } from 'vitest';

describe('GuestPrismaDataSource Unit Tests', () => {
  it('seharusnya menyertakan pencarian QRUID dan resolusi nama pengguna via tkuser untuk guestToUserName dan guestCheckOutByName', async () => {
    const mockFindManyUsers = vi.fn().mockImplementation(({ where }) => {
      if (where?.UserName?.contains) {
        return Promise.resolve([
          { UserEmail: 'wisnu@veranda.com', UserName: 'Wisnu Wijayadi' },
          { UserEmail: 'satpam@veranda.com', UserName: 'Wisnu Satpam' },
        ]);
      }
      if (where?.UserEmail?.in) {
        return Promise.resolve([
          { UserEmail: 'wisnu@veranda.com', UserName: 'Wisnu Wijayadi' },
          { UserEmail: 'satpam@veranda.com', UserName: 'Wisnu Satpam' },
        ]);
      }
      return Promise.resolve([]);
    });

    const mockFindManyGuests = vi.fn().mockResolvedValue([
      {
        GuestId: 1,
        GuestUID: 'GUEST-001',
        GuestGroupUID: 'GRP-001',
        GuestName: 'Bambang Sudarsono',
        GuestNumber: 'B 2706 SBX',
        GuestIdentity: '1234567890',
        GuestIdentityType: 'KTP',
        GuestPhoto: null,
        GuestDT: new Date('2026-09-17T10:00:00Z'),
        GuestAckDT: new Date('2026-09-17T10:05:00Z'),
        GuestOutDT: null,
        GuestPurpose: 'Antar Paket',
        GuestDestination: 'Blok A1',
        GuestToUserEmail: 'wisnu@veranda.com',
        GuestCreatorEmail: 'petugas@veranda.com',
        GuestCreatorName: 'Ahmad',
        GuestCategory: 'Kurir',
        QRUID: 'QR-VER-001',
        GuestAddress: 'Jakarta',
        GuestType: 'Delivery',
        GuestCheckOutByEmail: 'satpam@veranda.com',
      },
    ]);

    const mockFindManyUserGroups = vi
      .fn()
      .mockResolvedValue([{ Email: 'wisnu@veranda.com' }, { Email: 'satpam@veranda.com' }]);

    const mockDb = {
      tkguest: { findMany: mockFindManyGuests },
      tkuser: { findMany: mockFindManyUsers },
      tkusergroup: { findMany: mockFindManyUserGroups },
      tkgroup: {
        findMany: vi.fn().mockResolvedValue([{ GroupUID: 'GRP-001', GroupName: 'Veranda Group' }]),
      },
    } as unknown as PrismaClient;

    const ds = new GuestPrismaDataSource(mockDb);
    const result = await ds.getGuests('GRP-001', {
      search: 'Wisnu',
      limit: 10,
      offset: 0,
    });

    // 1. Verifikasi pengambilan anggota grup dan pencarian tkuser dengan nama 'Wisnu' ter-scoped
    expect(mockFindManyUserGroups).toHaveBeenCalledWith({
      where: {
        GroupUID: 'GRP-001',
        Status: 'JOINED',
      },
      select: { Email: true },
    });
    expect(mockFindManyUsers).toHaveBeenCalledWith({
      where: {
        UserEmail: { in: ['wisnu@veranda.com', 'satpam@veranda.com'] },
        UserName: { contains: 'Wisnu' },
      },
      select: { UserEmail: true },
    });

    // 2. Verifikasi klausa pencarian tkguest
    expect(mockFindManyGuests).toHaveBeenCalled();
    const callArgs = mockFindManyGuests.mock.calls[0]?.[0];
    expect(callArgs?.where?.GuestGroupUID).toBe('GRP-001');

    const orConditions = callArgs?.where?.OR;
    expect(orConditions).toContainEqual({ QRUID: { contains: 'Wisnu' } });
    expect(orConditions).toContainEqual({
      GuestToUserEmail: { in: ['wisnu@veranda.com', 'satpam@veranda.com'] },
    });
    expect(orConditions).toContainEqual({
      GuestCheckOutByEmail: { in: ['wisnu@veranda.com', 'satpam@veranda.com'] },
    });

    // 3. Verifikasi mapping entity hasil query
    expect(result).toHaveLength(1);
    const firstGuest = result[0];
    expect(firstGuest).toBeDefined();
    if (!firstGuest) return;

    expect(firstGuest.guestToUserName).toBe('Wisnu Wijayadi');
    expect(firstGuest.guestToUserEmail).toBe('wisnu@veranda.com');
    expect(firstGuest.guestCheckOutByName).toBe('Wisnu Satpam');
    expect(firstGuest.qrUid).toBe('QR-VER-001');

    // 4. Verifikasi GuestResponseMapper memiliki field guest_to_user_name
    const resObj = GuestResponseMapper.toResponse(firstGuest);
    expect(resObj.guest_to_user_name).toBe('Wisnu Wijayadi');
    expect(resObj.guest_to_user_email).toBe('wisnu@veranda.com');
    expect(resObj.qr_uid).toBe('QR-VER-001');
    expect(resObj.guest_checkout_by_name).toBe('Wisnu Satpam');
  });

  it('seharusnya membersihkan data kotor N/A, tanggal epoch 1970, dan mempertahankan status Menunggu jika checkout kotor (anti-poisoning & nullable)', async () => {
    const mockFindManyGuests = vi.fn().mockResolvedValue([
      {
        GuestId: 2,
        GuestUID: 'N/A',
        GuestGroupUID: 'GRP-001',
        GuestName: 'N/A',
        GuestNumber: 'N/A',
        GuestIdentity: 'N/A',
        GuestIdentityType: 'N/A',
        GuestPhoto: 'null',
        GuestDT: new Date('2026-09-17T10:00:00Z'),
        GuestAckDT: new Date('1970-01-01T00:00:00.000Z'),
        GuestOutDT: new Date('1970-01-01T00:00:00.000Z'),
        GuestPurpose: 'N/A',
        GuestDestination: 'N/A',
        GuestToUserEmail: 'N/A',
        GuestCreatorEmail: 'N/A',
        GuestCreatorName: 'N/A',
        GuestCategory: 'N/A',
        QRUID: 'N/A',
        GuestAddress: 'N/A',
        GuestType: 'N/A',
        GuestCheckOutByEmail: 'N/A',
      },
    ]);

    const mockDb = {
      tkguest: { findMany: mockFindManyGuests },
      tkuser: { findMany: vi.fn().mockResolvedValue([]) },
      tkgroup: { findMany: vi.fn().mockResolvedValue([]) },
    } as unknown as PrismaClient;

    const ds = new GuestPrismaDataSource(mockDb);
    const result = await ds.getGuests('GRP-001');

    expect(result).toHaveLength(1);
    const item = result[0];
    expect(item).toBeDefined();
    if (!item) return;

    // Field-field kotor 'N/A' dan 'null' harus menjadi null murni
    expect(item.guestUid).toBeNull();
    expect(item.guestName).toBeNull();
    expect(item.guestNumber).toBeNull();
    expect(item.guestIdentity).toBeNull();
    expect(item.guestIdentityType).toBeNull();
    expect(item.guestPhoto).toBeNull();
    expect(item.guestPurpose).toBeNull();
    expect(item.guestDestination).toBeNull();
    expect(item.guestToUserEmail).toBeNull();
    expect(item.guestCreatorEmail).toBeNull();
    expect(item.guestCreatorName).toBeNull();
    expect(item.guestCategory).toBeNull();
    expect(item.qrUid).toBeNull();
    expect(item.guestAddress).toBeNull();
    expect(item.guestType).toBeNull();
    expect(item.guestCheckOutByEmail).toBeNull();
    expect(item.guestCheckOutByName).toBeNull();

    // Tanggal epoch 1970 harus dibersihkan menjadi null
    expect(item.guestAckDt).toBeNull();
    expect(item.guestOutDt).toBeNull();
  });

  it('getGuests menyaring data tamu aktif (isActive true: GuestOutDT null) dan checkout (isActive false)', async () => {
    const mockFindMany = vi.fn().mockResolvedValue([]);
    const mockDb = {
      tkguest: { findMany: mockFindMany },
      tkuser: { findMany: vi.fn().mockResolvedValue([]) },
      tkgroup: { findMany: vi.fn().mockResolvedValue([]) },
    } as unknown as PrismaClient;

    const ds = new GuestPrismaDataSource(mockDb);

    // 1. isActive: true (Tamu Aktif / belum checkout)
    await ds.getGuests('GRP-001', { isActive: true });
    expect(mockFindMany).toHaveBeenCalledWith(
      expect.objectContaining({
        where: expect.objectContaining({
          GuestGroupUID: 'GRP-001',
          GuestOutDT: null,
        }),
      }),
    );

    // 2. isActive: false (Tamu sudah checkout)
    await ds.getGuests('GRP-001', { isActive: false });
    expect(mockFindMany).toHaveBeenCalledWith(
      expect.objectContaining({
        where: expect.objectContaining({
          GuestGroupUID: 'GRP-001',
          GuestOutDT: { not: null },
        }),
      }),
    );
  });

  it('createGuest berhasil melakukan insert ke database dan mengembalikan GuestEntity', async () => {
    const mockCreate = vi.fn().mockResolvedValue({
      GuestId: 99,
      GuestUID: 'GST-099',
      GuestGroupUID: 'GRP-001',
      GuestName: 'Tamu Baru',
      GuestNumber: 'B 1234 ABC',
      GuestIdentity: '320123456789',
      GuestIdentityType: 'KTP',
      GuestPhoto: null,
      GuestDT: new Date('2026-09-21T08:00:00Z'),
      GuestAckDT: null,
      GuestOutDT: null,
      GuestPurpose: 'Kunjungan',
      GuestDestination: 'Blok A',
      GuestToUserEmail: null,
      GuestCreatorEmail: 'petugas@veranda.com',
      GuestCreatorName: 'Petugas Satu',
      GuestCategory: 'Tamu',
      QRUID: 'GUEST-QR-001',
      GuestAddress: 'Depok',
      GuestType: 'TAMU',
      GuestCheckOutByEmail: null,
    });

    const mockDb = {
      tkguest: { create: mockCreate },
      tkuser: { findMany: vi.fn().mockResolvedValue([]) },
      tkgroup: { findMany: vi.fn().mockResolvedValue([]) },
    } as unknown as PrismaClient;

    const ds = new GuestPrismaDataSource(mockDb);
    const result = await ds.createGuest({
      system: {
        guestUid: 'GST-001',
        qrUid: 'GUEST-QR-001',
        guestCategory: 'Tamu',
        guestDt: new Date('2026-09-21T08:00:00Z'),
      },
      context: {
        groupUid: 'GRP-001',
        creatorEmail: 'petugas@veranda.com',
        creatorName: 'Petugas Satu',
      },
      data: {
        guestName: 'Tamu Baru',
        guestDestination: 'Blok A',
        guestPurpose: 'Kunjungan',
        guestType: 'TAMU',
      },
    });

    expect(result.id).toBe('99');
    expect(result.guestName).toBe('Tamu Baru');
    expect(mockCreate).toHaveBeenCalledWith(
      expect.objectContaining({
        data: expect.objectContaining({
          GuestCategory: 'Tamu',
          GuestType: 'TAMU',
          GuestAckDT: null,
          GuestOutDT: null,
          GuestCheckOutByEmail: null,
        }),
      }),
    );
  });

  it('getGuestById mengembalikan GuestEntity jika ditemukan dan null jika tidak', async () => {
    const mockFindFirst = vi.fn().mockImplementation(({ where }) => {
      const isMatch =
        where?.GuestId === 99 ||
        where?.OR?.some((cond: { GuestId?: number }) => cond.GuestId === 99);
      if (isMatch) {
        return Promise.resolve({
          GuestId: 99,
          GuestUID: 'GST-099',
          GuestGroupUID: 'GRP-001',
          GuestName: 'Tamu Ada',
          GuestDestination: 'Blok A',
          GuestType: 'TAMU',
          QRUID: 'QR-099',
          GuestDT: new Date(),
        });
      }
      return Promise.resolve(null);
    });

    const mockDb = {
      tkguest: { findFirst: mockFindFirst },
      tkuser: { findMany: vi.fn().mockResolvedValue([]) },
      tkgroup: { findMany: vi.fn().mockResolvedValue([]) },
    } as unknown as PrismaClient;

    const ds = new GuestPrismaDataSource(mockDb);
    const found = await ds.getGuestById('99', 'GRP-001');
    expect(found?.id).toBe('99');
    expect(found?.guestName).toBe('Tamu Ada');

    const notFound = await ds.getGuestById('100', 'GRP-001');
    expect(notFound).toBeNull();
  });

  it('updateGuest berhasil melakukan update record dan mengembalikan entity terupdate', async () => {
    const mockUpdate = vi.fn().mockResolvedValue({
      GuestId: 99,
      GuestUID: 'GST-099',
      GuestGroupUID: 'GRP-001',
      GuestName: 'Tamu Update',
      GuestDestination: 'Blok B',
      GuestType: 'TAMU',
      QRUID: 'QR-099',
      GuestDT: new Date(),
    });

    const mockDb = {
      tkguest: { update: mockUpdate },
      tkuser: { findMany: vi.fn().mockResolvedValue([]) },
      tkgroup: { findMany: vi.fn().mockResolvedValue([]) },
    } as unknown as PrismaClient;

    const ds = new GuestPrismaDataSource(mockDb);
    const result = await ds.updateGuest({
      context: { id: '99', groupUid: 'GRP-001' },
      data: {
        guestName: 'Tamu Update',
        guestDestination: 'Blok B',
      },
    });

    expect(result.id).toBe('99');
    expect(result.guestName).toBe('Tamu Update');
    expect(mockUpdate).toHaveBeenCalledWith(
      expect.objectContaining({
        where: { GuestId: 99 },
        data: expect.objectContaining({
          GuestName: 'Tamu Update',
          GuestDestination: 'Blok B',
        }),
      }),
    );
  });

  it('updateGuest berhasil melakukan update system fields (ackDt, outDt, checkOutByEmail)', async () => {
    const ackDate = new Date('2026-09-21T08:05:00.000Z');
    const outDate = new Date('2026-09-21T09:00:00.000Z');
    const mockUpdate = vi.fn().mockResolvedValue({
      GuestId: 99,
      GuestUID: 'GST-099',
      GuestGroupUID: 'GRP-001',
      GuestName: 'Tamu Update',
      GuestDestination: 'Blok B',
      GuestType: 'TAMU',
      QRUID: 'QR-099',
      GuestDT: new Date(),
      GuestAckDT: ackDate,
      GuestOutDT: outDate,
      GuestCheckOutByEmail: 'guard@veranda.com',
    });

    const mockDb = {
      tkguest: { update: mockUpdate },
      tkuser: { findMany: vi.fn().mockResolvedValue([]) },
      tkgroup: { findMany: vi.fn().mockResolvedValue([]) },
    } as unknown as PrismaClient;

    const ds = new GuestPrismaDataSource(mockDb);
    const result = await ds.updateGuest({
      context: { id: '99', groupUid: 'GRP-001', checkOutByEmail: 'guard@veranda.com' },
      system: {
        guestAckDt: ackDate,
        guestOutDt: outDate,
      },
    });

    expect(result.id).toBe('99');
    expect(mockUpdate).toHaveBeenCalledWith(
      expect.objectContaining({
        where: { GuestId: 99 },
        data: expect.objectContaining({
          GuestAckDT: ackDate,
          GuestOutDT: outDate,
          GuestCheckOutByEmail: 'guard@veranda.com',
        }),
      }),
    );
  });

  it('updateGuest melempar error jika ID bukan angka yang valid (NaN check)', async () => {
    const mockDb = {} as unknown as PrismaClient;

    const ds = new GuestPrismaDataSource(mockDb);

    await expect(
      ds.updateGuest({
        context: { id: 'invalid-id', groupUid: 'GRP-001' },
        data: {},
      }),
    ).rejects.toThrow('ID tamu harus berupa angka yang valid');
  });

  it('updateGuest dan createGuest menyimpan data yang sudah disanitasi ke DB tanpa sanitasi berulang', async () => {
    const mockCreate = vi.fn().mockResolvedValue({
      GuestId: 101,
      GuestUID: 'GST-101',
      GuestGroupUID: 'GRP-001',
      GuestName: 'Tamu Bersih',
      GuestDestination: 'Blok C',
      GuestType: 'TAMU',
      QRUID: 'GUEST-101',
      GuestDT: new Date(),
    });

    const mockFindFirst = vi.fn().mockResolvedValue({ GuestId: 101 });
    const mockUpdate = vi.fn().mockResolvedValue({
      GuestId: 101,
      GuestUID: 'GST-101',
      GuestGroupUID: 'GRP-001',
      GuestName: 'Tamu Bersih',
      GuestDestination: 'Blok C',
      GuestType: 'TAMU',
      QRUID: 'GUEST-101',
      GuestDT: new Date(),
    });

    const mockDb = {
      tkguest: { create: mockCreate, findFirst: mockFindFirst, update: mockUpdate },
      tkuser: { findMany: vi.fn().mockResolvedValue([]) },
      tkgroup: { findMany: vi.fn().mockResolvedValue([]) },
    } as unknown as PrismaClient;

    const ds = new GuestPrismaDataSource(mockDb);

    // createGuest menerima data bersih dari layer mapper/usecase
    await ds.createGuest({
      system: {
        guestUid: 'GST-101',
        qrUid: 'GUEST-CUSTOM-101',
        guestCategory: 'Umum',
        guestDt: new Date('2026-09-21T08:00:00Z'),
      },
      context: {
        groupUid: 'GRP-001',
        creatorEmail: 'guard@veranda.com',
        creatorName: 'Guard User',
      },
      data: {
        guestName: 'Tamu Bersih',
        guestDestination: 'Blok C',
        guestType: 'TAMU',
        guestPurpose: null,
        guestNumber: null,
        guestIdentity: null,
      },
    });

    expect(mockCreate).toHaveBeenCalledWith(
      expect.objectContaining({
        data: expect.objectContaining({
          GuestPurpose: null,
          GuestNumber: null,
          GuestIdentity: null,
          GuestCategory: 'Umum',
          GuestType: 'TAMU',
          QRUID: 'GUEST-CUSTOM-101',
        }),
      }),
    );

    // updateGuest menerima data bersih dari layer mapper/usecase
    await ds.updateGuest({
      context: {
        id: '101',
        groupUid: 'GRP-001',
      },
      data: {
        guestPurpose: null,
        guestNumber: null,
        guestIdentity: null,
      },
    });

    expect(mockUpdate).toHaveBeenCalledWith(
      expect.objectContaining({
        data: expect.objectContaining({
          GuestPurpose: null,
          GuestNumber: null,
          GuestIdentity: null,
        }),
      }),
    );
  });

  it('deleteManyGuests berhasil melakukan penghapusan record secara ter-scoped multi-tenant', async () => {
    const mockDeleteMany = vi.fn().mockResolvedValue({ count: 2 });
    const mockDb = {
      tkguest: { deleteMany: mockDeleteMany },
    } as unknown as PrismaClient;

    const ds = new GuestPrismaDataSource(mockDb);
    const result = await ds.deleteManyGuests(['99', '100'], 'GRP-001');
    expect(result).toBe(2);
    expect(mockDeleteMany).toHaveBeenCalledWith({
      where: { GuestId: { in: [99, 100] }, GuestGroupUID: 'GRP-001' },
    });

    // Mengembalikan 0 jika IDs kosong atau tidak ada yang valid
    expect(await ds.deleteManyGuests(['invalid-id', '-5'], 'GRP-001')).toBe(0);

    // Mengembalikan 0 jika record tidak ada dalam grup (count === 0)
    mockDeleteMany.mockResolvedValueOnce({ count: 0 });
    expect(await ds.deleteManyGuests(['100'], 'GRP-OTHER')).toBe(0);
  });

  it('resolveCanonicalGuestType mengembalikan GuestTypeName kanonikal jika ditemukan pada grup aktif', async () => {
    const mockFindFirst = vi.fn().mockResolvedValue({ GuestTypeName: 'TAMU RESMI' });
    const mockDb = {
      tkmasterguesttype: { findFirst: mockFindFirst },
    } as unknown as PrismaClient;

    const ds = new GuestPrismaDataSource(mockDb);
    const canonical = await ds.resolveCanonicalGuestType('tamu resmi', 'GRP-001');
    expect(canonical).toBe('TAMU RESMI');
    expect(mockFindFirst).toHaveBeenCalledWith({
      where: {
        GuestTypeGroupUID: 'GRP-001',
        OR: [{ GuestTypeName: 'tamu resmi' }, { GuestTypeUID: 'tamu resmi' }],
      },
      select: { GuestTypeName: true },
    });
  });

  it('isValidGuestType mengembalikan true jika guestType ditemukan pada grup aktif', async () => {
    const mockFindFirst = vi.fn().mockResolvedValue({ GuestTypeName: 'TAMU UMUM' });
    const mockDb = {
      tkmasterguesttype: { findFirst: mockFindFirst },
    } as unknown as PrismaClient;

    const ds = new GuestPrismaDataSource(mockDb);
    const isValid = await ds.isValidGuestType('TAMU UMUM', 'GRP-001');
    expect(isValid).toBe(true);
  });

  it('resolveCanonicalGuestType dan isValidGuestType mengembalikan false/null jika tidak ditemukan atau input kosong', async () => {
    const mockFindFirst = vi.fn().mockResolvedValue(null);
    const mockDb = {
      tkmasterguesttype: { findFirst: mockFindFirst },
    } as unknown as PrismaClient;

    const ds = new GuestPrismaDataSource(mockDb);
    expect(await ds.resolveCanonicalGuestType('', 'GRP-001')).toBeNull();
    expect(await ds.resolveCanonicalGuestType('UNKNOWN', '')).toBeNull();
    expect(await ds.resolveCanonicalGuestType('UNKNOWN', 'GRP-001')).toBeNull();
    expect(await ds.isValidGuestType('', 'GRP-001')).toBe(false);
    expect(await ds.isValidGuestType('UNKNOWN', '')).toBe(false);
    expect(await ds.isValidGuestType('UNKNOWN', 'GRP-001')).toBe(false);
  });

  it('getMasterGuestTypes mengembalikan daftar tipe tamu master untuk grup aktif dan membersihkan data kotor', async () => {
    const mockFindMany = vi.fn().mockResolvedValue([
      { GuestTypeId: 1, GuestTypeUID: '  UID-01  ', GuestTypeName: '  TAMU UMUM  ' },
      { GuestTypeId: 2, GuestTypeUID: 'UID-02', GuestTypeName: 'KURIR' },
      { GuestTypeId: 3, GuestTypeUID: 'N/A', GuestTypeName: 'KOTOR' },
      { GuestTypeId: 4, GuestTypeUID: 'UID-04', GuestTypeName: '   ' },
    ]);
    const mockDb = {
      tkmasterguesttype: { findMany: mockFindMany },
    } as unknown as PrismaClient;

    const ds = new GuestPrismaDataSource(mockDb);
    const types = await ds.getMasterGuestTypes('GRP-001');
    expect(types).toHaveLength(2);
    expect(types[0]).toEqual({ id: '1', uid: 'UID-01', name: 'TAMU UMUM' });
    expect(types[1]).toEqual({ id: '2', uid: 'UID-02', name: 'KURIR' });
    expect(mockFindMany).toHaveBeenCalledWith({
      where: { GuestTypeGroupUID: 'GRP-001' },
      orderBy: { GuestTypeName: 'asc' },
      select: {
        GuestTypeId: true,
        GuestTypeUID: true,
        GuestTypeName: true,
      },
    });
  });

  it('isValidHost mengembalikan true jika email terdaftar pada grup aktif', async () => {
    const mockFindFirst = vi.fn().mockResolvedValue({ UserGroupId: 1 });
    const mockDb = {
      tkusergroup: { findFirst: mockFindFirst },
    } as unknown as PrismaClient;

    const ds = new GuestPrismaDataSource(mockDb);
    const result = await ds.isValidHost('host@veranda.com', 'GRP-001');

    expect(result).toBe(true);
    expect(mockFindFirst).toHaveBeenCalledWith({
      where: {
        GroupUID: 'GRP-001',
        Email: 'host@veranda.com',
        Status: 'JOINED',
      },
      select: { UserGroupId: true },
    });
  });

  it('isValidHost mengembalikan false jika email tidak terdaftar pada grup aktif', async () => {
    const mockFindFirst = vi.fn().mockResolvedValue(null);
    const mockDb = {
      tkusergroup: { findFirst: mockFindFirst },
    } as unknown as PrismaClient;

    const ds = new GuestPrismaDataSource(mockDb);
    const notFoundResult = await ds.isValidHost('unknown@veranda.com', 'GRP-001');

    expect(notFoundResult).toBe(false);
    expect(mockFindFirst).toHaveBeenCalledWith({
      where: {
        GroupUID: 'GRP-001',
        Email: 'unknown@veranda.com',
        Status: 'JOINED',
      },
      select: { UserGroupId: true },
    });
  });

  describe('getGuestById', () => {
    it('mencari tamu berdasarkan ID numerik (GuestId) dalam grup aktif', async () => {
      const mockFindFirst = vi.fn().mockResolvedValue({
        GuestId: 10,
        GuestUID: 'GST-010',
        GuestGroupUID: 'GRP-001',
        GuestName: 'Pak Budi',
        GuestDestination: 'Blok A1',
        GuestType: 'TAMU',
        QRUID: 'QR-010',
        GuestDT: new Date('2026-09-20T10:00:00Z'),
      });
      const mockDb = {
        tkguest: { findFirst: mockFindFirst },
        tkuser: { findMany: vi.fn().mockResolvedValue([]) },
        tkgroup: { findMany: vi.fn().mockResolvedValue([]) },
      } as unknown as PrismaClient;

      const ds = new GuestPrismaDataSource(mockDb);
      const guest = await ds.getGuestById('10', 'GRP-001');

      expect(guest).not.toBeNull();
      expect(guest?.id).toBe('10');
      expect(mockFindFirst).toHaveBeenCalledWith(
        expect.objectContaining({
          where: {
            GuestGroupUID: 'GRP-001',
            OR: expect.arrayContaining([{ QRUID: '10' }, { GuestId: 10 }]),
          },
        }),
      );
    });

    it('mencari tamu berdasarkan QRUID dalam grup aktif', async () => {
      const mockFindFirst = vi.fn().mockResolvedValue({
        GuestId: 25,
        GuestUID: 'GST-025',
        GuestGroupUID: 'GRP-001',
        GuestName: 'Tamu Scan QR',
        GuestDestination: 'Blok C3',
        GuestType: 'TAMU',
        QRUID: 'c3d2b21b-4179-4d69-b541-6156e71ef3b5',
        GuestDT: new Date('2026-09-20T10:00:00Z'),
      });
      const mockDb = {
        tkguest: { findFirst: mockFindFirst },
        tkuser: { findMany: vi.fn().mockResolvedValue([]) },
        tkgroup: { findMany: vi.fn().mockResolvedValue([]) },
      } as unknown as PrismaClient;

      const ds = new GuestPrismaDataSource(mockDb);
      const guest = await ds.getGuestById('c3d2b21b-4179-4d69-b541-6156e71ef3b5', 'GRP-001');

      expect(guest).not.toBeNull();
      expect(guest?.id).toBe('25');
      expect(guest?.qrUid).toBe('c3d2b21b-4179-4d69-b541-6156e71ef3b5');
      expect(mockFindFirst).toHaveBeenCalledWith(
        expect.objectContaining({
          where: {
            GuestGroupUID: 'GRP-001',
            OR: [{ QRUID: 'c3d2b21b-4179-4d69-b541-6156e71ef3b5' }],
          },
        }),
      );
    });
  });
});
