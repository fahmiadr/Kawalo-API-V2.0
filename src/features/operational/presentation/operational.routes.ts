import { authMiddleware } from '@core/middleware/auth.middleware';
import { ApiResponseSchema } from '@core/utils/api-response';
import { operationalHandler } from '@features/operational/operational.module';
import { GetOperationalQueryDto } from '@features/operational/presentation/dtos/operational-query.dto';
import { GetOperationalResponseDto } from '@features/operational/presentation/dtos/operational-response.dto';
import { Elysia } from 'elysia';

export const operationalRoutes = new Elysia({ prefix: '/operational' })
  .use(authMiddleware)
  .get(
    '/',
    ({ query, currentUser, set }) =>
      operationalHandler.getOperationals({ query, currentUser: currentUser!, set }),
    {
      requireAuth: true,
      query: GetOperationalQueryDto,
      response: ApiResponseSchema(GetOperationalResponseDto),
    },
  );
