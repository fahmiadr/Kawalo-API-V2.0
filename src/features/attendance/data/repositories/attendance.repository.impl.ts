import { AttendancePrismaDataSource } from '@features/attendance/data/datasources/attendance-prisma.datasource';
import type { AttendanceEntity } from '@features/attendance/domain/entities/attendance.entity';
import type {
  AttendanceFilterParams,
  AttendanceRepository,
} from '@features/attendance/domain/repositories/attendance.repository';

export class AttendanceRepositoryImpl implements AttendanceRepository {
  constructor(
    private readonly prismaSource: AttendancePrismaDataSource = new AttendancePrismaDataSource(),
  ) {}

  public async getAttendances(filter?: AttendanceFilterParams): Promise<AttendanceEntity[]> {
    return this.prismaSource.getAttendances(filter);
  }
}
