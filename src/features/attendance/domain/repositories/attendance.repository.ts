import type { AttendanceEntity } from '@features/attendance/domain/entities/attendance.entity';

export interface AttendanceFilterParams {
  search?: string;
  limit?: number;
  offset?: number;
  startDate?: string;
  endDate?: string;
  groupUid?: string | null;
}

export interface AttendanceRepository {
  getAttendances(filter?: AttendanceFilterParams): Promise<AttendanceEntity[]>;
}
