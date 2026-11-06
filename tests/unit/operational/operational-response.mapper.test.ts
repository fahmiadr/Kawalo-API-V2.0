import {
  DutyTimelineEntity,
  OperationalEntity,
} from '@features/operational/domain/entities/operational.entity';
import { OperationalResponseMapper } from '@features/operational/presentation/mappers/operational-response.mapper';
import { describe, expect, it } from 'vitest';

describe('OperationalResponseMapper Unit Tests', () => {
  it('berhasil memetakan OperationalEntity ke OperationalItemDtoType', () => {
    const timeline = new DutyTimelineEntity({
      id: '1',
      journeyUid: 'JRN-001',
      duty: 'Patroli',
      state: 'Aktif',
      dutyDt: '2026-09-09T08:00:00.000Z',
      stateDt: '2026-09-09T08:30:00.000Z',
    });

    const entity = new OperationalEntity({
      id: 'satpam1@kawalo.id_2026-09-09',
      userEmail: 'satpam1@kawalo.id',
      userName: 'Budi Santoso',
      userProfilePic: 'https://cdn.kawalo.id/pic.jpg',
      groupName: 'Tim A',
      latestDuty: 'Patroli',
      latestState: 'Aktif',
      latestDutyDt: '2026-09-09T08:00:00.000Z',
      latestStateDt: '2026-09-09T08:30:00.000Z',
      totalActivities: 1,
      dutyDate: '2026-09-09',
      timeline: [timeline],
    });

    const response = OperationalResponseMapper.toResponse(entity);

    expect(response).toEqual({
      id: 'satpam1@kawalo.id_2026-09-09',
      user_email: 'satpam1@kawalo.id',
      user_name: 'Budi Santoso',
      user_profile_pic: 'https://cdn.kawalo.id/pic.jpg',
      group_name: 'Tim A',
      latest_duty: 'Patroli',
      latest_state: 'Aktif',
      latest_duty_dt: '2026-09-09T08:00:00.000Z',
      latest_state_dt: '2026-09-09T08:30:00.000Z',
      total_activities: 1,
      duty_date: '2026-09-09',
      timeline: [
        {
          id: '1',
          journey_uid: 'JRN-001',
          duty: 'Patroli',
          state: 'Aktif',
          duty_dt: '2026-09-09T08:00:00.000Z',
          state_dt: '2026-09-09T08:30:00.000Z',
        },
      ],
    });
  });

  it('berhasil memetakan daftar OperationalEntity via toResponseList', () => {
    const entity = new OperationalEntity({
      id: 'satpam1@kawalo.id_2026-09-09',
      userEmail: 'satpam1@kawalo.id',
      userName: 'Budi Santoso',
      totalActivities: 0,
      timeline: [],
    });

    const list = OperationalResponseMapper.toResponseList([entity]);
    expect(list).toHaveLength(1);
    expect(list[0]?.user_email).toBe('satpam1@kawalo.id');
    expect(list[0]?.user_name).toBe('Budi Santoso');
    expect(list[0]?.timeline).toEqual([]);
  });
});
