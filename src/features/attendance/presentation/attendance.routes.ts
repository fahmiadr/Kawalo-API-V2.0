import { authMiddleware } from '@core/middleware/auth.middleware';
import { ApiResponseSchema } from '@core/utils/api-response';
import { attendanceHandler } from '@features/attendance/attendance.module';
import { GetAttendanceQueryDto } from '@features/attendance/presentation/dtos/attendance-query.dto';
import { GetAttendanceResponseDto } from '@features/attendance/presentation/dtos/attendance-response.dto';
import { Elysia } from 'elysia';

export const attendanceRoutes = new Elysia({ prefix: '/attendance' })
  .use(authMiddleware)
  .get(
    '/',
    ({ query, currentUser, set }) =>
      attendanceHandler.getAttendances({ query, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      query: GetAttendanceQueryDto,
      response: ApiResponseSchema(GetAttendanceResponseDto),
    },
  );
