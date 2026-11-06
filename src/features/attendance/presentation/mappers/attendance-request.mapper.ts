import type { AuthenticatedUserPayload } from '@core/types/jwt-payload.types';
import { cleanQueryDate, cleanQueryString } from '@core/utils/clean-data.util';
import { resolvePagination } from '@core/utils/pagination.util';
import type { GetAttendanceQueryDtoType } from '@features/attendance/presentation/dtos/attendance-query.dto';
import type { GetAttendancesInput } from '@features/attendance/usecases/get-attendances.usecase';

export class AttendanceRequestMapper {
  public static toGetAttendancesInput(
    query: GetAttendanceQueryDtoType,
    currentUser: AuthenticatedUserPayload,
  ): GetAttendancesInput {
    const { take, skip } = resolvePagination({
      limit: query.limit,
      offset: query.offset,
    });

    return {
      search: cleanQueryString(query.search),
      limit: take,
      offset: skip,
      startDate: cleanQueryDate(query.start_date),
      endDate: cleanQueryDate(query.end_date),
      currentUser,
    };
  }
}
