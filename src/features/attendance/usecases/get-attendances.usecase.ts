import type { Failure } from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type { AttendanceEntity } from '@features/attendance/domain/entities/attendance.entity';
import type { AttendanceRepository } from '@features/attendance/domain/repositories/attendance.repository';

export interface GetAttendancesInput {
  currentUser: { group_uid?: string | null };
  search?: string;
  limit?: number;
  offset?: number;
  startDate?: string;
  endDate?: string;
}

export class GetAttendancesUseCase implements UseCase<AttendanceEntity[], GetAttendancesInput> {
  constructor(private readonly repository: AttendanceRepository) {}

  public async execute(input: GetAttendancesInput): Promise<Result<AttendanceEntity[], Failure>> {
    if (!input.currentUser.group_uid) {
      return Result.ok([]);
    }

    const attendances = await this.repository.getAttendances({
      search: input.search,
      limit: input.limit,
      offset: input.offset,
      startDate: input.startDate,
      endDate: input.endDate,
      groupUid: input.currentUser.group_uid,
    });
    return Result.ok(attendances);
  }
}
