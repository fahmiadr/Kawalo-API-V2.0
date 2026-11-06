import { AppConstants } from '@core/constants/app.constants';
import type { AuthenticatedUserPayload } from '@core/types/jwt-payload.types';
import { AttendanceRequestMapper } from '@features/attendance/presentation/mappers/attendance-request.mapper';
import { describe, expect, it } from 'vitest';

describe('AttendanceRequestMapper Unit Tests', () => {
  const dummyUser: AuthenticatedUserPayload = {
    sub: 'usr-att',
    email: 'att@kawalo.id',
    group_uid: 'GRP-001',
    session_ver: 1,
  };

  it('berhasil membersihkan input query dan session ke GetAttendancesInput yang valid', () => {
    const query = {
      search: '  Budi Santoso  ',
      limit: 10,
      offset: 20,
      start_date: '2026-09-01T00:00:00.000Z',
      end_date: '2026-09-10T23:59:59.000Z',
    };

    const input = AttendanceRequestMapper.toGetAttendancesInput(query, dummyUser);

    expect(input.search).toBe('Budi Santoso');
    expect(input.limit).toBe(10);
    expect(input.offset).toBe(20);
    expect(input.startDate).toBe('2026-09-01T00:00:00.000Z');
    expect(input.endDate).toBe('2026-09-10T23:59:59.000Z');
    expect(input.currentUser.group_uid).toBe('GRP-001');
  });

  it('mengonversi string kosong, whitespace, null, atau N/A menjadi undefined', () => {
    const query = {
      search: '   ',
      start_date: 'null',
      end_date: 'N/A',
    };
    const userWithoutGroup: AuthenticatedUserPayload = {
      ...dummyUser,
      group_uid: undefined,
    };

    const input = AttendanceRequestMapper.toGetAttendancesInput(query, userWithoutGroup);

    expect(input.search).toBeUndefined();
    expect(input.startDate).toBeUndefined();
    expect(input.endDate).toBeUndefined();
    expect(input.currentUser.group_uid).toBeUndefined();
  });

  it('menyelesaikan paginasi default jika limit dan offset tidak diisi', () => {
    const query = {};

    const input = AttendanceRequestMapper.toGetAttendancesInput(query, dummyUser);

    expect(input.limit).toBe(AppConstants.DEFAULT_PAGE_SIZE);
    expect(input.offset).toBe(0);
  });

  it('menangani limit 0 sebagai query tanpa batas (take: undefined)', () => {
    const query = { limit: 0 };

    const input = AttendanceRequestMapper.toGetAttendancesInput(query, dummyUser);

    expect(input.limit).toBeUndefined();
    expect(input.offset).toBeUndefined();
  });
});
