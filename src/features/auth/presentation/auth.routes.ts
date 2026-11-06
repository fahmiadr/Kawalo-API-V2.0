import { authMiddleware } from '@core/middleware/auth.middleware';
import { ApiResponseSchema } from '@core/utils/api-response';
import { authHandler } from '@features/auth/auth.module';
import {
  LoginDto,
  LogoutDto,
  RefreshTokenDto,
  SwitchGroupDto,
} from '@features/auth/presentation/dtos/auth-request.dto';
import {
  JwksResponseDto,
  LoginResponseDto,
  LogoutResponseDto,
  SwitchGroupResponseDto,
  TokensResponseDto,
} from '@features/auth/presentation/dtos/auth-response.dto';
import { Elysia } from 'elysia';

export const authRoutes = new Elysia({ prefix: '/auth' })
  .use(authMiddleware)
  .post('/login', ({ body, set, request }) => authHandler.login({ body, set, request }), {
    body: LoginDto,
    response: ApiResponseSchema(LoginResponseDto),
  })
  .post('/refresh', ({ body, set }) => authHandler.refresh({ body, set }), {
    body: RefreshTokenDto,
    response: ApiResponseSchema(TokensResponseDto),
  })
  .post(
    '/switch-group',
    ({ body, set, currentUser }) =>
      authHandler.switchGroup({ body, set, currentUser: currentUser! }),
    {
      requireAuth: true,
      body: SwitchGroupDto,
      response: ApiResponseSchema(SwitchGroupResponseDto),
    },
  )
  .post('/logout', ({ body, set, currentUser }) => authHandler.logout({ body, set, currentUser }), {
    body: LogoutDto,
    response: ApiResponseSchema(LogoutResponseDto),
  });

export const jwksRoute = new Elysia().get(
  '/.well-known/jwks.json',
  ({ set }) => authHandler.jwks({ set }),
  {
    response: {
      200: JwksResponseDto,
    },
  },
);
