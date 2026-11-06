import { AttendancePrismaDataSource } from '@features/attendance/data/datasources/attendance-prisma.datasource';
import type { PrismaClient } from '@generated/prisma/client';
import { describe, expect, it, vi } from 'vitest';

describe('AttendancePrismaDataSource Unit Tests', () => {
  it('seharusnya mengembalikan array kosong jika groupUid tidak ada (multi-tenant guard)', async () => {
    const mockDb = {
      tkabsen: { findMany: vi.fn() },
      tkshift: { findMany: vi.fn() },
    } as unknown as PrismaClient;

    const ds = new AttendancePrismaDataSource(mockDb);
    const result = await ds.getAttendances();

    expect(result).toEqual([]);
    expect(mockDb.tkabsen.findMany).not.toHaveBeenCalled();
  });

  it('seharusnya menyertakan pencarian nama shift via tkshift dan memetakan AShiftUID ke whereClause', async () => {
    const mockFindManyShifts = vi
      .fn()
      .mockResolvedValue([{ ShiftUID: 'SHIFT-MORNING-01' }, { ShiftUID: 'SHIFT-MORNING-02' }]);

    const mockFindManyAbsen = vi.fn().mockResolvedValue([
      {
        AbsenID: 1,
        AbsenEmail: 'satpam1@kawalo.id',
        AbsenName: 'Budi Santoso',
        AbsenDT: new Date('2026-09-17T07:00:00Z'),
        AbsenType: 'CHECK IN',
        ALat: '-6.200000',
        ALong: '106.816666',
        ADiffMins: 0,
        ALocation: 'Pos Utama',
        AShiftUID: 'SHIFT-MORNING-01',
        AbsenGroupUID: 'GRP-001',
        AShStart: '07:00:00',
      },
    ]);

    const mockDb = {
      tkabsen: { findMany: mockFindManyAbsen },
      tkshift: {
        findMany: mockFindManyShifts,
      },
      tkgroup: {
        findMany: vi.fn().mockResolvedValue([{ GroupUID: 'GRP-001', GroupName: 'Cluster A' }]),
      },
      tkuser: { findMany: vi.fn().mockResolvedValue([]) },
    } as unknown as PrismaClient;

    const ds = new AttendancePrismaDataSource(mockDb);
    const result = await ds.getAttendances({
      groupUid: 'GRP-001',
      search: 'Pagi',
      limit: 10,
      offset: 0,
    });

    // Verifikasi tkshift dipanggil dengan ShiftGroupUID dan ShiftName contains 'Pagi'
    expect(mockFindManyShifts).toHaveBeenCalledWith({
      where: {
        ShiftGroupUID: 'GRP-001',
        ShiftName: { contains: 'Pagi' },
      },
      select: { ShiftUID: true },
    });

    // Verifikasi whereClause tkabsen menyertakan AShiftUID in matchingShiftUids
    expect(mockFindManyAbsen).toHaveBeenCalledWith(
      expect.objectContaining({
        where: expect.objectContaining({
          AbsenGroupUID: 'GRP-001',
          OR: [
            { AbsenName: { contains: 'Pagi' } },
            { AbsenEmail: { contains: 'Pagi' } },
            { AbsenType: { contains: 'Pagi' } },
            { ALocation: { contains: 'Pagi' } },
            { AShiftUID: { in: ['SHIFT-MORNING-01', 'SHIFT-MORNING-02'] } },
          ],
        }),
      }),
    );

    expect(result).toHaveLength(1);
    expect(result[0]?.userName).toBe('Budi Santoso');
  });

  it('seharusnya membersihkan data kotor, tidak menghalusinasikan timestamp dan tipe absensi (anti-corruption & anti-hallucination)', async () => {
    const mockFindManyAbsen = vi.fn().mockResolvedValue([
      {
        AbsenID: 99,
        AbsenEmail: 'N/A',
        AbsenName: 'N/A',
        AbsenDT: new Date('1970-01-01T00:00:00Z'), // Tanggal korup/epoch 1970
        AbsenType: 'N/A',
        ALat: '0.0',
        ALong: '0.0',
        ADiffMins: 0,
        ALocation: 'N/A',
        AShiftUID: 'N/A',
        AbsenGroupUID: 'GRP-001',
        AShStart: '00:00:00',
      },
    ]);

    const mockDb = {
      tkabsen: { findMany: mockFindManyAbsen },
      tkshift: { findMany: vi.fn().mockResolvedValue([]) },
      tkgroup: { findMany: vi.fn().mockResolvedValue([]) },
      tkuser: { findMany: vi.fn().mockResolvedValue([]) },
    } as unknown as PrismaClient;

    const ds = new AttendancePrismaDataSource(mockDb);
    const result = await ds.getAttendances({ groupUid: 'GRP-001' });

    expect(result).toHaveLength(1);
    const item = result[0];

    // AbsenDT korup/epoch 1970 bernilai null murni tanpa rekayasa
    expect(item?.attendanceDt).toBeNull();

    // AbsenEmail & AbsenName bernilai 'N/A' dibersihkan menjadi null tanpa fallback '-'
    expect(item?.userEmail).toBeNull();
    expect(item?.userName).toBeNull();

    // AbsenType 'N/A' dibersihkan menjadi null tanpa memaksakan 'REGISTER'
    expect(item?.attendanceType).toBeNull();
    expect(item?.category).toBe('lainnya');

    // Koordinat 0.0 dibersihkan menjadi null
    expect(item?.latitude).toBeNull();
    expect(item?.longitude).toBeNull();

    // ALocation dan AShiftUID 'N/A' dibersihkan menjadi null
    expect(item?.locationName).toBeNull();
    expect(item?.shiftTimeWindow).toBeNull();
  });

  it('seharusnya mengabaikan record absensi jika AbsenID korup atau bukan integer valid (anti-hallucination)', async () => {
    const mockFindManyAbsen = vi.fn().mockResolvedValue([
      {
        AbsenID: 'not-a-valid-id' as any,
        AbsenEmail: 'user@kawalo.id',
        AbsenName: 'User Corrupt ID',
        AbsenDT: new Date('2026-09-17T07:00:00Z'),
        AbsenType: 'CHECK IN',
        ALat: '-6.200000',
        ALong: '106.816666',
        ADiffMins: 0,
        ALocation: 'Pos A',
        AShiftUID: null,
        AbsenGroupUID: 'GRP-001',
        AShStart: null,
      },
      {
        AbsenID: null as any,
        AbsenEmail: 'user2@kawalo.id',
        AbsenName: 'User Null ID',
        AbsenDT: new Date('2026-09-17T07:00:00Z'),
        AbsenType: 'CHECK IN',
        ALat: '-6.200000',
        ALong: '106.816666',
        ADiffMins: 0,
        ALocation: 'Pos B',
        AShiftUID: null,
        AbsenGroupUID: 'GRP-001',
        AShStart: null,
      },
      {
        AbsenID: 10,
        AbsenEmail: 'user3@kawalo.id',
        AbsenName: 'Valid User',
        AbsenDT: new Date('2026-09-17T07:00:00Z'),
        AbsenType: 'CHECK IN',
        ALat: '-6.200000',
        ALong: '106.816666',
        ADiffMins: 0,
        ALocation: 'Pos C',
        AShiftUID: null,
        AbsenGroupUID: 'GRP-001',
        AShStart: null,
      },
    ]);

    const mockDb = {
      tkabsen: { findMany: mockFindManyAbsen },
      tkshift: { findMany: vi.fn().mockResolvedValue([]) },
      tkgroup: { findMany: vi.fn().mockResolvedValue([]) },
      tkuser: { findMany: vi.fn().mockResolvedValue([]) },
    } as unknown as PrismaClient;

    const ds = new AttendancePrismaDataSource(mockDb);
    const result = await ds.getAttendances({ groupUid: 'GRP-001' });

    // Hanya record dengan AbsenID integer valid (10) yang dipetakan
    expect(result).toHaveLength(1);
    expect(result[0]?.id).toBe('10');
    expect(result[0]?.userName).toBe('Valid User');
  });
});
