import { AppConstants } from '@core/constants/app.constants';
import type { AuthenticatedUserPayload } from '@core/types/jwt-payload.types';
import { OperationalRequestMapper } from '@features/operational/presentation/mappers/operational-request.mapper';
import { describe, expect, it } from 'vitest';

describe('OperationalRequestMapper Unit Tests', () => {
  const dummyUser: AuthenticatedUserPayload = {
    sub: 'usr-ops',
    email: 'ops@kawalo.id',
    group_uid: 'GRP-OPS-01',
    session_ver: 1,
  };

  it('berhasil membersihkan input query dan session ke GetOperationalInput yang valid', () => {
    const query = {
      search: '  Patroli Blok B  ',
      limit: 10,
      offset: 20,
      start_date: '2026-09-01T00:00:00.000Z',
      end_date: '2026-09-10T23:59:59.000Z',
    };

    const input = OperationalRequestMapper.toGetOperationalInput(query, dummyUser);

    expect(input.search).toBe('Patroli Blok B');
    expect(input.limit).toBe(10);
    expect(input.offset).toBe(20);
    expect(input.startDate).toBe('2026-09-01T00:00:00.000Z');
    expect(input.endDate).toBe('2026-09-10T23:59:59.000Z');
    expect(input.currentUser.group_uid).toBe('GRP-OPS-01');
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

    const input = OperationalRequestMapper.toGetOperationalInput(query, userWithoutGroup);

    expect(input.search).toBeUndefined();
    expect(input.startDate).toBeUndefined();
    expect(input.endDate).toBeUndefined();
    expect(input.currentUser.group_uid).toBeUndefined();
  });

  it('menyelesaikan paginasi default jika limit dan offset tidak diisi', () => {
    const query = {};

    const input = OperationalRequestMapper.toGetOperationalInput(query, dummyUser);

    expect(input.limit).toBe(AppConstants.DEFAULT_PAGE_SIZE);
    expect(input.offset).toBe(0);
  });

  it('menangani limit 0 sebagai query tanpa batas (take: undefined)', () => {
    const query = { limit: 0 };

    const input = OperationalRequestMapper.toGetOperationalInput(query, dummyUser);

    expect(input.limit).toBeUndefined();
    expect(input.offset).toBeUndefined();
  });
});
