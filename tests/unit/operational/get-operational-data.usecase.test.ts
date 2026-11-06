import {
  DutyTimelineEntity,
  OperationalEntity,
} from '@features/operational/domain/entities/operational.entity';
import type {
  OperationalFilterParams,
  OperationalRepository,
} from '@features/operational/domain/repositories/operational.repository';
import { OperationalResponseMapper } from '@features/operational/presentation/mappers/operational-response.mapper';
import { GetOperationalDataUseCase } from '@features/operational/usecases/get-operational-data.usecase';
import { describe, expect, it } from 'vitest';

class MockOperationalRepository implements OperationalRepository {
  public data: OperationalEntity[] = [];

  async getOperationals(_filter?: OperationalFilterParams): Promise<OperationalEntity[]> {
    return this.data;
  }
}

describe('GetOperationalDataUseCase Unit Tests', () => {
  it('berhasil mengembalikan daftar pekerja operasional beserta timeline aktivitas', async () => {
    const mockRepo = new MockOperationalRepository();
    const operational = new OperationalEntity({
      id: 'satpam1@kawalo.id_2026-09-09',
      userEmail: 'satpam1@kawalo.id',
      userName: 'Budi Santoso',
      groupName: 'Tim Pengamanan Blok A',
      latestDuty: 'Patroli',
      latestState: 'Aktif',
      latestDutyDt: '2026-09-09T08:00:00.000Z',
      latestStateDt: '2026-09-09T08:30:00.000Z',
      totalActivities: 2,
      timeline: [
        new DutyTimelineEntity({
          id: '1',
          journeyUid: 'JRN-001',
          duty: 'Patroli',
          state: 'Aktif',
          dutyDt: '2026-09-09T08:00:00.000Z',
          stateDt: '2026-09-09T08:30:00.000Z',
        }),
        new DutyTimelineEntity({
          id: '2',
          journeyUid: 'JRN-002',
          duty: 'Standby',
          state: 'Selesai',
          dutyDt: '2026-09-09T07:00:00.000Z',
          stateDt: '2026-09-09T07:55:00.000Z',
        }),
      ],
    });
    mockRepo.data = [operational];

    const usecase = new GetOperationalDataUseCase(mockRepo);
    const result = await usecase.execute({
      currentUser: { group_uid: 'group-security-a' },
    });

    expect(result.isSuccess()).toBe(true);
    const operationals = result.value;
    expect(operationals).toHaveLength(1);
    expect(operationals[0]?.userEmail).toBe('satpam1@kawalo.id');
    expect(operationals[0]?.userName).toBe('Budi Santoso');
    expect(operationals[0]?.latestDuty).toBe('Patroli');
    expect(operationals[0]?.timeline).toHaveLength(2);
    expect(operationals[0]?.timeline[0]?.journeyUid).toBe('JRN-001');

    // Verifikasi pemetaan Presentation via OperationalResponseMapper
    const firstItem = operationals[0];
    expect(firstItem).toBeDefined();
    if (!firstItem) throw new Error('Data operasional tidak ditemukan');
    const mapped = OperationalResponseMapper.toResponse(firstItem);
    expect(mapped.user_email).toBe('satpam1@kawalo.id');
    expect(mapped.user_name).toBe('Budi Santoso');
    expect(mapped.latest_duty).toBe('Patroli');
    expect(mapped.timeline[0]?.journey_uid).toBe('JRN-001');
  });

  it('mengembalikan array kosong jika belum ada data tenaga kerja di grup aktif', async () => {
    const mockRepo = new MockOperationalRepository();
    mockRepo.data = [];

    const usecase = new GetOperationalDataUseCase(mockRepo);
    const result = await usecase.execute({
      currentUser: { group_uid: 'empty-group' },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value).toEqual([]);
  });

  it('mengembalikan array kosong jika pengguna belum memilih grup aktif (group_uid null atau undefined)', async () => {
    const mockRepo = new MockOperationalRepository();
    mockRepo.data = [
      new OperationalEntity({
        id: 'satpam1@kawalo.id_2026-09-09',
        userEmail: 'satpam1@kawalo.id',
        totalActivities: 0,
        timeline: [],
      }),
    ];

    const usecase = new GetOperationalDataUseCase(mockRepo);

    const resultNull = await usecase.execute({
      currentUser: { group_uid: null },
    });
    expect(resultNull.isSuccess()).toBe(true);
    expect(resultNull.value).toEqual([]);

    const resultUndefined = await usecase.execute({
      currentUser: { group_uid: undefined },
    });
    expect(resultUndefined.isSuccess()).toBe(true);
    expect(resultUndefined.value).toEqual([]);
  });
});
