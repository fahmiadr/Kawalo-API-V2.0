import { AttendanceEntity } from '@features/attendance/domain/entities/attendance.entity';
import type {
  AttendanceFilterParams,
  AttendanceRepository,
} from '@features/attendance/domain/repositories/attendance.repository';
import { AttendanceResponseMapper } from '@features/attendance/presentation/mappers/attendance-response.mapper';
import { GetAttendancesUseCase } from '@features/attendance/usecases/get-attendances.usecase';
import { describe, expect, it } from 'vitest';

class MockAttendanceRepository implements AttendanceRepository {
  public data: AttendanceEntity[] = [];

  async getAttendances(_filter?: AttendanceFilterParams): Promise<AttendanceEntity[]> {
    return this.data;
  }
}

describe('GetAttendancesUseCase Unit Tests', () => {
  it('berhasil mengembalikan daftar data absensi dengan kategori yang benar', async () => {
    const mockRepo = new MockAttendanceRepository();
    const item1 = new AttendanceEntity({
      id: '101',
      userEmail: 'guard1@kawalo.id',
      userName: 'Ahmad Santoso',
      userProfilePic: 'https://example.com/pic1.jpg',
      userDuty: 'ON DUTY',
      attendanceDt: '2026-09-10T07:05:00.000Z',
      attendanceType: 'Absen Masuk shift pagi',
      latitude: '-6.200000',
      longitude: '106.816666',
      diffMinutes: 5,
      locationName: 'Pos Barat',
      shiftName: 'Shift Pagi',
      shiftColor: '#10B981',
      shiftTimeWindow: '07:00 - 15:00',
      groupName: 'Cluster Edelweiss',
    });

    const item2 = new AttendanceEntity({
      id: '102',
      userEmail: 'guard2@kawalo.id',
      userName: 'Bambang Sudiro',
      userProfilePic: null,
      userDuty: 'OFF DUTY',
      attendanceDt: '2026-09-10T15:02:00.000Z',
      attendanceType: 'CHECK OUT',
      latitude: '-6.201000',
      longitude: '106.817000',
      diffMinutes: 0,
      locationName: 'Pos Utama',
      shiftName: 'Shift Pagi',
      shiftColor: '#10B981',
      shiftTimeWindow: '07:00 - 15:00',
      groupName: 'Cluster Edelweiss',
    });

    const item3 = new AttendanceEntity({
      id: '103',
      userEmail: 'guard3@kawalo.id',
      userName: 'Charlie Putra',
      userProfilePic: null,
      userDuty: 'STANDBY',
      attendanceDt: '2026-09-10T12:00:00.000Z',
      attendanceType: 'REGISTER',
      latitude: null,
      longitude: null,
      diffMinutes: 0,
      locationName: 'Pos Patroli C',
      shiftName: null,
      shiftColor: null,
      shiftTimeWindow: null,
      groupName: 'Cluster Edelweiss',
    });

    mockRepo.data = [item1, item2, item3];

    const useCase = new GetAttendancesUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: { group_uid: 'group-kawalo-1' },
    });

    expect(result.isSuccess()).toBe(true);
    const items = result.value;
    expect(items).toHaveLength(3);

    expect(items[0]?.id).toBe('101');
    expect(items[0]?.userName).toBe('Ahmad Santoso');
    expect(items[0]?.category).toBe('masuk');
    expect(items[0]?.shiftTimeWindow).toBe('07:00 - 15:00');
    expect(items[0]?.diffMinutes).toBe(5);

    // Verifikasi pemetaan ke Presentation DTO via ResponseMapper
    const firstItem = items[0];
    expect(firstItem).toBeDefined();
    if (!firstItem) throw new Error('Item tidak ditemukan');
    const mapped = AttendanceResponseMapper.toResponse(firstItem);
    expect(mapped.user_name).toBe('Ahmad Santoso');
    expect(mapped.shift_time_window).toBe('07:00 - 15:00');
    expect(mapped.diff_minutes).toBe(5);

    expect(items[1]?.id).toBe('102');
    expect(items[1]?.category).toBe('pulang');

    expect(items[2]?.id).toBe('103');
    expect(items[2]?.category).toBe('lainnya');
    expect(items[2]?.latitude).toBeNull();
  });

  it('mengembalikan array kosong jika belum ada data absensi', async () => {
    const mockRepo = new MockAttendanceRepository();
    mockRepo.data = [];

    const useCase = new GetAttendancesUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: { group_uid: 'empty-group' },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value).toEqual([]);
  });

  it('mengembalikan array kosong jika pengguna belum memilih grup aktif (group_uid null atau whitespace)', async () => {
    const mockRepo = new MockAttendanceRepository();
    mockRepo.data = [
      new AttendanceEntity({
        id: '101',
        diffMinutes: 0,
      }),
    ];

    const useCase = new GetAttendancesUseCase(mockRepo);
    const resultNull = await useCase.execute({
      currentUser: { group_uid: null },
    });
    expect(resultNull.isSuccess()).toBe(true);
    expect(resultNull.value).toEqual([]);

    const resultEmpty = await useCase.execute({
      currentUser: { group_uid: undefined },
    });
    expect(resultEmpty.isSuccess()).toBe(true);
    expect(resultEmpty.value).toEqual([]);
  });
});
