import { InformationPrismaDataSource } from '@features/information/data/datasources/information-prisma.datasource';
import type { PrismaClient } from '@generated/prisma/client';
import { describe, expect, it, vi } from 'vitest';

describe('InformationPrismaDataSource Unit Tests', () => {
  it('seharusnya memetakan data dengan 100% pra-sanitasi, zero hallucination (pure nullable), dan safe map lookup', async () => {
    const mockFeedRecords = [
      {
        FeedId: 201,
        FeedUID: 'FEED-201',
        FeedTitle: 'Informasi Penjagaan',
        FeedText: 'Jadwal apel pagi diundur 30 menit.',
        FeedByEmail: 'Chief.Security@Kawalo.ID', // Mixed case email
        FeedByName: null, // Kosong di tkfeed, harus fallback ke tkuser
        FeedDT: new Date('2026-09-18T08:00:00Z'),
        FeedFile: 'sop.pdf',
        FeedPic: 'https://cdn.kawalo.id/photo1.jpg',
        FeedPic2: 'N/A', // Dirty string, harus dieliminasi dari array photos
        FeedPic3: 'https://cdn.kawalo.id/photo3.jpg',
        FeedPic4: '   ', // Whitespace, harus dieliminasi dari array photos
        FeedType: 'ANNOUNCEMENT',
        FeedByProfilePic: 'N/A', // Dirty string, harus fallback ke tkuser
        FeedGroupUID: 'GRP-SEC-01',
        FeedGroupName: null, // Kosong di tkfeed, harus fallback ke tkgroup
        FeedEventLocation: 'Posko Barat',
        FeedEventStartDT: '0000-00-00 00:00:00', // Invalid MySQL epoch date, harus jadi null
        FeedEventEndDT: new Date('2026-09-18T10:00:00Z'),
        FeedEventDuration: 60,
        GLat: '-6.200000',
        GLong: '106.816666',
      },
      {
        FeedId: 202,
        FeedUID: 'FEED-202',
        FeedTitle: 'N/A', // Dirty title, harus jadi null (Zero Hallucination)
        FeedText: null,
        FeedByEmail: 'staff@kawalo.id',
        FeedByName: 'Staff Lapangan',
        FeedDT: new Date('2026-09-18T07:00:00Z'),
        FeedFile: null,
        FeedPic: null,
        FeedPic2: 'N/A',
        FeedPic3: 'N/A',
        FeedPic4: 'N/A',
        FeedType: null,
        FeedByProfilePic: 'https://cdn.kawalo.id/staff.png',
        FeedGroupUID: 'GRP-SEC-01',
        FeedGroupName: 'Security Kawalo',
        FeedEventLocation: 'N/A', // Dirty location, harus jadi null
        FeedEventStartDT: null,
        FeedEventEndDT: null,
        FeedEventDuration: null,
        GLat: 'N/A', // Dirty coord, harus jadi null
        GLong: 'N/A', // Dirty coord, harus jadi null
      },
    ];

    const mockDb = {
      tkfeed: {
        findMany: vi.fn().mockResolvedValue(mockFeedRecords),
      },
      tkgroup: {
        findMany: vi.fn().mockResolvedValue([
          {
            GroupUID: 'GRP-SEC-01',
            GroupName: 'Grup Pengamanan Terpadu',
          },
        ]),
      },
      tkuser: {
        findMany: vi.fn().mockResolvedValue([
          {
            UserEmail: 'chief.security@kawalo.id',
            UserName: 'Kapten Hendra',
            UserProfilePic: 'https://cdn.kawalo.id/profiles/hendra.jpg',
          },
        ]),
      },
    } as unknown as PrismaClient;

    const ds = new InformationPrismaDataSource(mockDb);
    const results = await ds.getInformation('GRP-SEC-01');

    expect(results).toHaveLength(2);

    // Verifikasi item 1: Fallback relasional aman & sanitasi array foto
    const item1 = results[0];
    expect(item1).toBeDefined();
    if (!item1) throw new Error('Item 1 tidak ditemukan');

    expect(item1.id).toBe('201');
    expect(item1.title).toBe('Informasi Penjagaan');
    expect(item1.text).toBe('Jadwal apel pagi diundur 30 menit.');
    expect(item1.userEmail).toBe('chief.security@kawalo.id'); // Otomatis lowercase
    expect(item1.userName).toBe('Kapten Hendra'); // Fallback dari userMap
    expect(item1.userProfilePic).toBe('https://cdn.kawalo.id/profiles/hendra.jpg'); // Fallback dari userMap
    expect(item1.groupName).toBe('Grup Pengamanan Terpadu'); // Fallback dari groupMap
    expect(item1.photos).toEqual([
      'https://cdn.kawalo.id/photo1.jpg',
      'https://cdn.kawalo.id/photo3.jpg',
    ]);
    expect(item1.eventLocation).toBe('Posko Barat');
    expect(item1.eventStartDt).toBeNull(); // 0000-00-00 dinormalisasi ke null murni
    expect(item1.eventEndDt).toBe('2026-09-18T10:00:00.000Z');
    expect(item1.eventDuration).toBe(60);
    expect(item1.glat).toBe('-6.200000');
    expect(item1.glong).toBe('106.816666');

    // Verifikasi item 2: Zero data hallucination (pure nullables)
    const item2 = results[1];
    expect(item2).toBeDefined();
    if (!item2) throw new Error('Item 2 tidak ditemukan');

    expect(item2.id).toBe('202');
    expect(item2.title).toBeNull(); // 'N/A' dinormalisasi ke null tanpa fake string
    expect(item2.photos).toEqual([]); // Semua dirty string dieliminasi
    expect(item2.eventLocation).toBeNull(); // 'N/A' dinormalisasi ke null
    expect(item2.glat).toBeNull(); // 'N/A' dinormalisasi ke null
    expect(item2.glong).toBeNull(); // 'N/A' dinormalisasi ke null

    // Verifikasi pemanggilan findMany menggunakan explicit select (tanpa NetStatus & FeedEventCal)
    expect(mockDb.tkfeed.findMany).toHaveBeenCalledWith(
      expect.objectContaining({
        where: { FeedGroupUID: 'GRP-SEC-01' },
        select: expect.not.objectContaining({
          NetStatus: true,
          FeedEventCal: true,
        }),
      }),
    );
  });

  it('seharusnya mengembalikan array kosong dan tidak memanggil relasi jika tidak ada record feed ditemukan', async () => {
    const mockDb = {
      tkfeed: {
        findMany: vi.fn().mockResolvedValue([]),
      },
      tkgroup: { findMany: vi.fn() },
      tkuser: { findMany: vi.fn() },
    } as unknown as PrismaClient;

    const ds = new InformationPrismaDataSource(mockDb);
    const results = await ds.getInformation('GRP-EMPTY-01');

    expect(results).toEqual([]);
    expect(mockDb.tkfeed.findMany).toHaveBeenCalledTimes(1);
    expect(mockDb.tkgroup.findMany).not.toHaveBeenCalled();
    expect(mockDb.tkuser.findMany).not.toHaveBeenCalled();
  });

  it('createInformation berhasil membuat record di database dan mengembalikan InformationEntity', async () => {
    const mockCreate = vi.fn().mockResolvedValue({
      FeedId: 301,
      FeedUID: 'FEED-301',
      FeedTitle: 'Info Baru',
      FeedText: 'Deskripsi Info',
      FeedByEmail: 'petugas@veranda.com',
      FeedByName: 'Petugas Satu',
      FeedDT: new Date('2026-09-21T08:00:00Z'),
      FeedFile: null,
      FeedPic: 'https://cdn.veranda.com/pic1.jpg',
      FeedPic2: 'N/A',
      FeedPic3: 'N/A',
      FeedPic4: 'N/A',
      FeedType: 'INFO',
      FeedByProfilePic: null,
      FeedGroupUID: 'GRP-01',
      FeedGroupName: 'Cluster Sakura',
      FeedEventLocation: null,
      FeedEventStartDT: null,
      FeedEventEndDT: null,
      FeedEventDuration: null,
      GLat: 'N/A',
      GLong: 'N/A',
    });

    const mockDb = {
      tkfeed: { create: mockCreate },
      tkuser: { findMany: vi.fn().mockResolvedValue([]) },
      tkgroup: { findMany: vi.fn().mockResolvedValue([]) },
    } as unknown as PrismaClient;

    const ds = new InformationPrismaDataSource(mockDb);
    const result = await ds.createInformation({
      system: {
        feedUid: 'feed_018f3a5b-9d48-7abc-89ef-0123456789ab',
        feedGroupName: 'Veranda Group',
        feedByProfilePic: 'https://cdn.veranda.com/profile.jpg',
        feedDt: new Date(),
      },
      context: {
        groupUid: 'GRP-01',
        creatorEmail: 'petugas@veranda.com',
        creatorName: 'Petugas Satu',
      },
      data: {
        title: 'Info Baru',
        feedType: 'EVENT',
        text: 'Deskripsi Info',
        feedPic: 'https://cdn.veranda.com/pic1.jpg',
      },
    });

    expect(result.id).toBe('301');
    expect(result.title).toBe('Info Baru');
    expect(result.photos).toEqual(['https://cdn.veranda.com/pic1.jpg']);
    expect(mockCreate).toHaveBeenCalledWith(
      expect.objectContaining({
        data: expect.objectContaining({
          FeedUID: 'feed_018f3a5b-9d48-7abc-89ef-0123456789ab',
          FeedGroupUID: 'GRP-01',
          FeedByEmail: 'petugas@veranda.com',
          FeedByName: 'Petugas Satu',
          FeedGroupName: 'Veranda Group',
          FeedByProfilePic: 'https://cdn.veranda.com/profile.jpg',
          FeedPic: 'https://cdn.veranda.com/pic1.jpg',
        }),
      }),
    );
  });

  it('getFeedCreatorContext mengembalikan groupName dan userProfilePic yang tersanitasi', async () => {
    const mockDb = {
      tkgroup: {
        findUnique: vi.fn().mockResolvedValue({ GroupName: '  Perumahan Veranda  ' }),
      },
      tkuser: {
        findUnique: vi
          .fn()
          .mockResolvedValue({ UserProfilePic: '  https://cdn.veranda.com/avatar.jpg  ' }),
      },
    } as unknown as PrismaClient;

    const ds = new InformationPrismaDataSource(mockDb);
    const context = await ds.getFeedCreatorContext('GRP-01', 'admin@veranda.com');

    expect(context.groupName).toBe('Perumahan Veranda');
    expect(context.userProfilePic).toBe('https://cdn.veranda.com/avatar.jpg');
  });

  it('getInformationById mengembalikan InformationEntity jika ditemukan dan null jika tidak', async () => {
    const mockFindFirst = vi.fn().mockImplementation(({ where }) => {
      if (where?.FeedId === 301 && where?.FeedGroupUID === 'GRP-01') {
        return Promise.resolve({
          FeedId: 301,
          FeedUID: 'FEED-301',
          FeedTitle: 'Ada Info',
          FeedText: 'Teks',
          FeedByEmail: 'admin@veranda.com',
          FeedByName: 'Admin',
          FeedDT: new Date(),
          FeedGroupUID: 'GRP-01',
          GLat: 'N/A',
          GLong: 'N/A',
        });
      }
      return Promise.resolve(null);
    });

    const mockDb = {
      tkfeed: { findFirst: mockFindFirst },
      tkuser: { findMany: vi.fn().mockResolvedValue([]) },
      tkgroup: { findMany: vi.fn().mockResolvedValue([]) },
    } as unknown as PrismaClient;

    const ds = new InformationPrismaDataSource(mockDb);
    const found = await ds.getInformationById('301', 'GRP-01');
    expect(found?.id).toBe('301');
    expect(found?.title).toBe('Ada Info');

    const notFound = await ds.getInformationById('999', 'GRP-01');
    expect(notFound).toBeNull();
  });

  it('updateInformation berhasil memperbarui record dan mengembalikan entity terupdate', async () => {
    const mockUpdate = vi.fn().mockResolvedValue({
      FeedId: 301,
      FeedUID: 'FEED-301',
      FeedTitle: 'Judul Baru',
      FeedText: 'Teks Baru',
      FeedByEmail: 'admin@veranda.com',
      FeedByName: 'Admin',
      FeedDT: new Date(),
      FeedGroupUID: 'GRP-01',
      GLat: 'N/A',
      GLong: 'N/A',
    });

    const mockDb = {
      tkfeed: { update: mockUpdate },
      tkuser: { findMany: vi.fn().mockResolvedValue([]) },
      tkgroup: { findMany: vi.fn().mockResolvedValue([]) },
    } as unknown as PrismaClient;

    const ds = new InformationPrismaDataSource(mockDb);
    const result = await ds.updateInformation({
      context: { id: '301', groupUid: 'GRP-01' },
      data: {
        title: 'Judul Baru',
        text: 'Teks Baru',
      },
    });

    expect(result.id).toBe('301');
    expect(result.title).toBe('Judul Baru');
    expect(mockUpdate).toHaveBeenCalledWith(
      expect.objectContaining({
        where: { FeedId: 301, FeedGroupUID: 'GRP-01' },
        data: expect.objectContaining({
          FeedTitle: 'Judul Baru',
          FeedText: 'Teks Baru',
        }),
      }),
    );
  });

  it('getInformationByIds mengembalikan daftar entities sesuai IDs dan groupUid', async () => {
    const mockFindMany = vi.fn().mockResolvedValue([
      {
        FeedId: 301,
        FeedUID: 'FEED-301',
        FeedTitle: 'Info 1',
        FeedText: 'Teks 1',
        FeedByEmail: 'admin@veranda.com',
        FeedGroupUID: 'GRP-01',
      },
      {
        FeedId: 302,
        FeedUID: 'FEED-302',
        FeedTitle: 'Info 2',
        FeedText: 'Teks 2',
        FeedByEmail: 'admin@veranda.com',
        FeedGroupUID: 'GRP-01',
      },
    ]);

    const mockDb = {
      tkfeed: { findMany: mockFindMany },
      tkuser: { findMany: vi.fn().mockResolvedValue([]) },
      tkgroup: { findMany: vi.fn().mockResolvedValue([]) },
    } as unknown as PrismaClient;

    const ds = new InformationPrismaDataSource(mockDb);
    const results = await ds.getInformationByIds(['301', '302'], 'GRP-01');

    expect(results).toHaveLength(2);
    expect(results[0]?.id).toBe('301');
    expect(results[1]?.id).toBe('302');
    expect(mockFindMany).toHaveBeenCalledWith(
      expect.objectContaining({
        where: {
          FeedId: { in: [301, 302] },
          FeedGroupUID: 'GRP-01',
        },
      }),
    );
  });

  it('getInformationByIds mengembalikan array kosong jika IDs kosong atau tidak valid', async () => {
    const mockDb = {
      tkfeed: { findMany: vi.fn() },
    } as unknown as PrismaClient;

    const ds = new InformationPrismaDataSource(mockDb);
    const res1 = await ds.getInformationByIds([], 'GRP-01');
    const res2 = await ds.getInformationByIds(['abc', '0'], 'GRP-01');

    expect(res1).toEqual([]);
    expect(res2).toEqual([]);
    expect(mockDb.tkfeed.findMany).not.toHaveBeenCalled();
  });

  it('deleteManyInformation mengeksekusi deleteMany dan mengembalikan jumlah yang dihapus', async () => {
    const mockDeleteMany = vi.fn().mockResolvedValue({ count: 3 });
    const mockDb = {
      tkfeed: { deleteMany: mockDeleteMany },
    } as unknown as PrismaClient;

    const ds = new InformationPrismaDataSource(mockDb);
    const count = await ds.deleteManyInformation(['301', '302', '303'], 'GRP-01');

    expect(count).toBe(3);
    expect(mockDeleteMany).toHaveBeenCalledWith({
      where: {
        FeedId: { in: [301, 302, 303] },
        FeedGroupUID: 'GRP-01',
      },
    });
  });

  it('deleteManyInformation mengembalikan 0 jika IDs kosong atau tidak valid', async () => {
    const mockDb = {
      tkfeed: { deleteMany: vi.fn() },
    } as unknown as PrismaClient;

    const ds = new InformationPrismaDataSource(mockDb);
    const count1 = await ds.deleteManyInformation([], 'GRP-01');
    const count2 = await ds.deleteManyInformation(['abc', '0'], 'GRP-01');

    expect(count1).toBe(0);
    expect(count2).toBe(0);
  });

  it('createInformation membuat data baru ke database dan mengembalikan InformationEntity', async () => {
    const mockCreated = {
      FeedId: 501,
      FeedUID: 'FEED-501',
      FeedTitle: 'Informasi Baru',
      FeedText: 'Deskripsi informasi',
      FeedByEmail: 'admin@kawalo.id',
      FeedByName: 'Admin',
      FeedDT: new Date('2026-09-18T08:00:00Z'),
      FeedFile: null,
      FeedPic: 'https://cdn.kawalo.id/photo1.jpg',
      FeedPic2: 'N/A',
      FeedPic3: 'N/A',
      FeedPic4: 'N/A',
      FeedType: 'GENERAL',
      FeedByProfilePic: null,
      FeedGroupUID: 'GRP-01',
      FeedGroupName: 'Cluster Anggrek',
      FeedEventLocation: null,
      FeedEventStartDT: null,
      FeedEventEndDT: null,
      FeedEventDuration: null,
      GLat: 'N/A',
      GLong: 'N/A',
    };

    const mockDb = {
      tkfeed: {
        create: vi.fn().mockResolvedValue(mockCreated),
      },
      tkgroup: {
        findMany: vi.fn().mockResolvedValue([]),
      },
      tkuser: {
        findMany: vi.fn().mockResolvedValue([]),
      },
    } as unknown as PrismaClient;

    const ds = new InformationPrismaDataSource(mockDb);
    const result = await ds.createInformation({
      system: {
        feedUid: 'feed_018f3a5b-9d48-7abc-89ef-0123456789ab',
        feedDt: new Date(),
      },
      context: {
        groupUid: 'GRP-01',
        creatorEmail: 'admin@kawalo.id',
        creatorName: 'Admin',
      },
      data: {
        title: 'Informasi Baru',
        text: 'Deskripsi informasi',
        feedType: 'GENERAL',
        feedPic: 'https://cdn.kawalo.id/photo1.jpg',
      },
    });

    expect(result.id).toBe('501');
    expect(result.title).toBe('Informasi Baru');
    expect(result.userEmail).toBe('admin@kawalo.id');
    expect(mockDb.tkfeed.create).toHaveBeenCalledWith(
      expect.objectContaining({
        data: expect.objectContaining({
          FeedGroupUID: 'GRP-01',
          FeedByEmail: 'admin@kawalo.id',
          FeedByName: 'Admin',
          FeedTitle: 'Informasi Baru',
          FeedText: 'Deskripsi informasi',
          FeedType: 'GENERAL',
          FeedPic: 'https://cdn.kawalo.id/photo1.jpg',
        }),
      }),
    );
  });

  it('updateInformation memperbarui data di database dan mengembalikan InformationEntity', async () => {
    const mockUpdated = {
      FeedId: 501,
      FeedUID: 'FEED-501',
      FeedTitle: 'Informasi Diperbarui',
      FeedText: 'Deskripsi baru',
      FeedByEmail: 'admin@kawalo.id',
      FeedByName: 'Admin',
      FeedDT: new Date('2026-09-18T08:00:00Z'),
      FeedFile: null,
      FeedPic: 'https://cdn.kawalo.id/photo2.jpg',
      FeedPic2: 'N/A',
      FeedPic3: 'N/A',
      FeedPic4: 'N/A',
      FeedType: 'EVENT',
      FeedByProfilePic: null,
      FeedGroupUID: 'GRP-01',
      FeedGroupName: 'Cluster Anggrek',
      FeedEventLocation: null,
      FeedEventStartDT: null,
      FeedEventEndDT: null,
      FeedEventDuration: null,
      GLat: 'N/A',
      GLong: 'N/A',
    };

    const mockDb = {
      tkfeed: {
        update: vi.fn().mockResolvedValue(mockUpdated),
      },
      tkgroup: {
        findMany: vi.fn().mockResolvedValue([]),
      },
      tkuser: {
        findMany: vi.fn().mockResolvedValue([]),
      },
    } as unknown as PrismaClient;

    const ds = new InformationPrismaDataSource(mockDb);
    const result = await ds.updateInformation({
      context: { id: '501', groupUid: 'GRP-01' },
      data: {
        title: 'Informasi Diperbarui',
        text: 'Deskripsi baru',
        feedType: 'EVENT',
        feedPic: 'https://cdn.kawalo.id/photo2.jpg',
      },
    });

    expect(result.id).toBe('501');
    expect(result.title).toBe('Informasi Diperbarui');
    expect(mockDb.tkfeed.update).toHaveBeenCalledWith(
      expect.objectContaining({
        where: { FeedId: 501, FeedGroupUID: 'GRP-01' },
        data: expect.objectContaining({
          FeedTitle: 'Informasi Diperbarui',
          FeedText: 'Deskripsi baru',
          FeedType: 'EVENT',
          FeedPic: 'https://cdn.kawalo.id/photo2.jpg',
        }),
      }),
    );
  });
});
