import { authMiddleware } from '@core/middleware/auth.middleware';
import { ApiResponseSchema } from '@core/utils/api-response';
import { groupHandler } from '@features/group/group.module';
import { GetGroupMembersResponseDto } from '@features/group/presentation/dtos/group-response.dto';
import { Elysia } from 'elysia';

/**
 * `requireAuth: true` pada authMiddleware menjamin `currentUser` non-null sebelum
 * handler dijalankan.
 */
export const groupRoutes = new Elysia({ prefix: '/groups' })
  .use(authMiddleware)
  .get(
    '/members',
    ({ currentUser, set }) => groupHandler.getMembers({ currentUser: currentUser!, set }),
    {
      requireAuth: true,
      response: ApiResponseSchema(GetGroupMembersResponseDto),
    },
  );
