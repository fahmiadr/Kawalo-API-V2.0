import { AppConstants } from '@core/constants/app.constants';
import { SessionVersionService } from '@core/security/session-version.service';
import { TokenService } from '@core/security/token.service';
import type { AuthenticatedUserPayload } from '@core/types/jwt-payload.types';
import { ApiResponse } from '@core/utils/api-response';
import { Elysia } from 'elysia';

export const authMiddleware = new Elysia({ name: 'auth-middleware' })
  .derive(
    { as: 'scoped' },
    async ({
      headers,
      request,
    }): Promise<{
      currentUser: AuthenticatedUserPayload | null;
      isSessionStale: boolean;
    }> => {
      const authHeader =
        headers.authorization ||
        headers.Authorization ||
        request?.headers?.get?.('authorization') ||
        request?.headers?.get?.('Authorization') ||
        undefined;

      const authPrefix = `${AppConstants.AUTH_TOKEN_TYPE} `;
      if (!authHeader?.startsWith(authPrefix)) {
        return { currentUser: null, isSessionStale: false };
      }

      const token = authHeader.substring(authPrefix.length).trim();
      if (!token) {
        return { currentUser: null, isSessionStale: false };
      }

      const user = await TokenService.verifyAccessToken(token);
      if (!user || !user.sub || !user.email) {
        return { currentUser: null, isSessionStale: false };
      }

      const isSessionValid = await SessionVersionService.validateOrRehydrate(
        user.sub,
        user.session_ver,
      );

      if (!isSessionValid) {
        return { currentUser: null, isSessionStale: true };
      }

      return { currentUser: user as AuthenticatedUserPayload, isSessionStale: false };
    },
  )
  .macro({
    requireAuth(enabled = true) {
      if (!enabled) return {};
      return {
        beforeHandle({ currentUser, isSessionStale, set }) {
          if (!currentUser) {
            set.status = 'Unauthorized';
            if (isSessionStale) {
              return ApiResponse.error(
                'Sesi telah diperbarui di sistem. Silakan perbarui sesi Anda.',
                ApiResponse.errorCode.AUTH_SESSION_STALE,
              );
            }
            return ApiResponse.error(
              'Akses tidak diizinkan. Token tidak valid atau telah kedaluwarsa.',
              ApiResponse.errorCode.AUTH_UNAUTHORIZED,
            );
          }
        },
      };
    },
    requireRole(roles: string[]) {
      const allowedRoles = roles.map((r) => r.trim().toUpperCase());
      return {
        beforeHandle({ currentUser, isSessionStale, set }) {
          if (!currentUser) {
            set.status = 'Unauthorized';
            if (isSessionStale) {
              return ApiResponse.error(
                'Sesi telah diperbarui di sistem. Silakan perbarui sesi Anda.',
                ApiResponse.errorCode.AUTH_SESSION_STALE,
              );
            }
            return ApiResponse.error(
              'Akses tidak diizinkan. Silakan login terlebih dahulu.',
              ApiResponse.errorCode.AUTH_UNAUTHORIZED,
            );
          }
          if (!currentUser.role || !allowedRoles.includes(currentUser.role)) {
            set.status = 'Forbidden';
            return ApiResponse.error(
              'Anda tidak memiliki wewenang untuk mengakses sumber daya ini.',
              ApiResponse.errorCode.AUTH_FORBIDDEN,
            );
          }
        },
      };
    },
  });
