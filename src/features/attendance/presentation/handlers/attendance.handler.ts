import type { AuthenticatedHandlerContext } from '@core/types/context.types';
import { ApiResponse } from '@core/utils/api-response';
import type { GetAttendanceQueryDtoType } from '@features/attendance/presentation/dtos/attendance-query.dto';
import { AttendanceRequestMapper } from '@features/attendance/presentation/mappers/attendance-request.mapper';
import { AttendanceResponseMapper } from '@features/attendance/presentation/mappers/attendance-response.mapper';
import type { GetAttendancesUseCase } from '@features/attendance/usecases/get-attendances.usecase';

export interface GetAttendanceContext extends AuthenticatedHandlerContext {
  query: GetAttendanceQueryDtoType;
}

export class AttendanceHandler {
  constructor(private readonly getAttendancesUseCase: GetAttendancesUseCase) {}

  public async getAttendances({ query, currentUser, set }: GetAttendanceContext) {
    const input = AttendanceRequestMapper.toGetAttendancesInput(query, currentUser);
    const result = await this.getAttendancesUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    const responseData = AttendanceResponseMapper.toResponseList(result.value);
    return ApiResponse.success(responseData, 'Daftar data absensi berhasil diambil');
  }
}
