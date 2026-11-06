import { OperationalPrismaDataSource } from '@features/operational/data/datasources/operational-prisma.datasource';
import type { PrismaClient } from '@generated/prisma/client';
import { describe, expect, it, vi } from 'vitest';

describe('OperationalPrismaDataSource Unit Tests', () => {
  it('seharusnya mengembalikan array kosong jika groupUid tidak ada atau falsy (multi-tenant guard)', async () => {
    const mockDb = {
      $queryRaw: vi.fn(),
      tkuser: { findMany: vi.fn() },
    } as unknown as PrismaClient;

    const ds = new OperationalPrismaDataSource(mockDb);
    const result = await ds.getOperationals();

    expect(result).toEqual([]);
    expect(mockDb.$queryRaw).not.toHaveBeenCalled();
  });

  it('seharusnya memetakan data dengan 100% pra-sanitasi, zero hallucination (pure nullable), dan safe map lookup', async () => {
    const mockRawDistinctDays = [
      {
        userEmail: 'satpam1@kawalo.id',
        dutyDate: '2026-09-18',
        maxDT: new Date('2026-09-18T10:00:00Z'),
      },
    ];

    const mockRecords = [
      {
        DutyJourneyId: 101,
        DutyJourneyUID: 'JRN-101',
        DutyGroupUID: 'GRP-OPS-01',
        DutyGroupName: 'Security Tim A',
        DutyDuty: 'Patroli',
        DutyState: 'Selesai',
        DutyDutyDT: new Date('2026-09-18T10:00:00Z'),
        DutyStateDT: '0000-00-00 00:00:00', // Legacy invalid epoch, harus jadi null (Zero Hallucination)
        UserEmail: 'satpam1@kawalo.id',
        UserName: 'Budi Santoso',
        dutyDate: '2026-09-18',
      },
      {
        DutyJourneyId: 100,
        DutyJourneyUID: 'JRN-100',
        DutyGroupUID: 'GRP-OPS-01',
        DutyGroupName: 'Security Tim A',
        DutyDuty: 'N/A', // Dirty string, harus jadi null
        DutyState: 'Standby',
        DutyDutyDT: new Date('2026-09-18T08:00:00Z'),
        DutyStateDT: new Date('2026-09-18T09:30:00Z'),
        UserEmail: 'satpam1@kawalo.id',
        UserName: 'Budi Santoso',
        dutyDate: '2026-09-18',
      },
    ];

    const mockQueryRaw = vi
      .fn()
      .mockResolvedValueOnce(mockRawDistinctDays)
      .mockResolvedValueOnce(mockRecords);

    const mockDb = {
      $queryRaw: mockQueryRaw,
      tkuser: {
        findMany: vi.fn().mockResolvedValue([
          {
            UserEmail: 'satpam1@kawalo.id',
            UserProfilePic: 'https://cdn.kawalo.id/profiles/budi.jpg',
          },
        ]),
      },
    } as unknown as PrismaClient;

    const ds = new OperationalPrismaDataSource(mockDb);
    const operationals = await ds.getOperationals({
      groupUid: 'GRP-OPS-01',
    });

    expect(operationals).toHaveLength(1);
    const operational = operationals[0];
    expect(operational).toBeDefined();
    if (!operational) throw new Error('Data operasional tidak ditemukan');

    expect(operational.userEmail).toBe('satpam1@kawalo.id');
    expect(operational.userName).toBe('Budi Santoso');
    expect(operational.userProfilePic).toBe('https://cdn.kawalo.id/profiles/budi.jpg');
    expect(operational.groupName).toBe('Security Tim A');
    expect(operational.latestDuty).toBe('Patroli');
    expect(operational.latestState).toBe('Selesai');
    expect(operational.latestDutyDt).toBe('2026-09-18T10:00:00.000Z');
    expect(operational.latestStateDt).toBeNull(); // Terbukti dinormalisasi ke null murni
    expect(operational.totalActivities).toBe(2);
    expect(operational.dutyDate).toBe('2026-09-18');

    // Verifikasi timeline
    expect(operational.timeline).toHaveLength(2);
    expect(operational.timeline[0]?.journeyUid).toBe('JRN-101');
    expect(operational.timeline[0]?.stateDt).toBeNull(); // 0000-00-00 dinormalisasi ke null
    expect(operational.timeline[1]?.duty).toBeNull(); // 'N/A' dinormalisasi ke null
    expect(operational.timeline[1]?.stateDt).toBe('2026-09-18T09:30:00.000Z');
  });

  it('seharusnya memfilter timeline item yang tidak memiliki id valid dan menangani limit tanpa offset dengan aman', async () => {
    const mockRawDistinctDays = [
      {
        userEmail: 'satpam2@kawalo.id',
        dutyDate: '2026-09-19',
        maxDT: new Date('2026-09-19T08:00:00Z'),
      },
    ];

    const mockRecords = [
      {
        DutyJourneyId: null as unknown as number,
        DutyJourneyUID: '   ', // UID kosong, Id null -> Harus difilter (Anti-Hallucination)
        DutyGroupUID: 'GRP-OPS-01',
        DutyGroupName: 'Security Tim A',
        DutyDuty: 'Patroli',
        DutyState: 'Aktif',
        DutyDutyDT: new Date('2026-09-19T08:00:00Z'),
        DutyStateDT: null,
        UserEmail: 'satpam2@kawalo.id',
        UserName: 'Santoso',
        dutyDate: '2026-09-19',
      },
      {
        DutyJourneyId: 202,
        DutyJourneyUID: 'JRN-202',
        DutyGroupUID: 'GRP-OPS-01',
        DutyGroupName: 'Security Tim A',
        DutyDuty: 'Standby',
        DutyState: 'Selesai',
        DutyDutyDT: new Date('2026-09-19T07:00:00Z'),
        DutyStateDT: null,
        UserEmail: 'satpam2@kawalo.id',
        UserName: 'Santoso',
        dutyDate: '2026-09-19',
      },
    ];

    const mockDb = {
      $queryRaw: vi
        .fn()
        .mockResolvedValueOnce(mockRawDistinctDays)
        .mockResolvedValueOnce(mockRecords),
      tkuser: { findMany: vi.fn().mockResolvedValue([]) },
    } as unknown as PrismaClient;

    const ds = new OperationalPrismaDataSource(mockDb);
    // Panggil dengan limit tanpa offset
    const operationals = await ds.getOperationals({
      groupUid: 'GRP-OPS-01',
      limit: 10,
    });

    expect(operationals).toHaveLength(1);
    // Hanya item kedua (202) yang memiliki ID valid
    expect(operationals[0]?.timeline).toHaveLength(1);
    expect(operationals[0]?.timeline[0]?.id).toBe('202');
  });
});
