import { EnvConfig } from '@core/config/env.config';
import { AppConstants } from '@core/constants/app.constants';
import { PrismaService } from '@core/database/prisma.service';
import { RedisService } from '@core/database/redis.service';
import { errorHandlerMiddleware } from '@core/middleware/error-handler.middleware';
import { requestLoggerMiddleware } from '@core/middleware/request-logger.middleware';
import { ApiResponse } from '@core/utils/api-response';
import { resolveClientIp } from '@core/utils/client-ip.util';
import { cors } from '@elysiajs/cors';
import { nodeAdapter } from '@core/adapter/node.adapter';
import { attendanceRoutes } from '@features/attendance/presentation/attendance.routes';
import { authRoutes, jwksRoute } from '@features/auth/presentation/auth.routes';
import { groupRoutes } from '@features/group/presentation/group.routes';
import { guestRoutes } from '@features/guest/presentation/guest.routes';
import { incidentRoutes } from '@features/incident/presentation/incident.routes';
import { informationRoutes } from '@features/information/presentation/information.routes';
import { operationalRoutes } from '@features/operational/presentation/operational.routes';
import { pttWsRoute } from '@features/ptt/presentation/ptt.ws';
import { Elysia, StatusMap } from 'elysia';
import { rateLimit } from 'elysia-rate-limit';

export const app = new Elysia({ adapter: nodeAdapter })
  // Centralized Request Logger & Correlation ID Middleware
  .use(requestLoggerMiddleware)

  // Rate Limiter Plugin (Elysia Ecosystem)
  .use(
    rateLimit({
      duration: EnvConfig.RATE_LIMIT_DURATION,
      max: EnvConfig.RATE_LIMIT_MAX,
      generator: (req) => resolveClientIp(req),
      errorResponse: new Response(
        JSON.stringify(
          ApiResponse.error(
            'Terlalu banyak permintaan. Silakan coba beberapa saat lagi.',
            ApiResponse.errorCode.RATE_LIMIT_EXCEEDED,
          ),
        ),
        {
          status: StatusMap['Too Many Requests'],
          headers: { 'Content-Type': 'application/json' },
        },
      ),
    }),
  )

  // CORS Plugin (Elysia Ecosystem)
  .use(
    cors({
      origin: EnvConfig.CORS_ORIGIN === '*' ? true : EnvConfig.CORS_ORIGIN.split(','),
      methods: ['GET', 'POST', 'PUT', 'PATCH', 'DELETE', 'OPTIONS'],
      allowedHeaders: ['Content-Type', 'Authorization', 'x-request-id'],
      exposeHeaders: ['x-request-id'],
      credentials: true,
    }),
  )

  // Security Header Enforcement
  .onAfterHandle({ as: 'global' }, ({ set }) => {
    set.headers['x-content-type-options'] = 'nosniff';
    set.headers['x-frame-options'] = 'DENY';
  })

  // Centralized Error Handling Middleware
  .use(errorHandlerMiddleware)

  // System Health Check (Liveness Probe: proses Node.js aktif)
  .get('/health', () => {
    return ApiResponse.success(
      {
        app: AppConstants.APP_NAME,
        status: 'ok',
        uptime: process.uptime(),
        timestamp: new Date().toISOString(),
      },
      'Sistem berjalan normal',
    );
  })

  // System Readiness Check (Readiness Probe: verifikasi live MariaDB & Redis)
  .get('/health/ready', async ({ set }) => {
    const [dbHealth, redisHealth] = await Promise.all([
      PrismaService.healthCheck(),
      RedisService.healthCheck(),
    ]);

    const isReady = dbHealth.status === 'healthy' && redisHealth.status === 'healthy';

    if (!isReady) {
      set.status = 'Service Unavailable';
      return ApiResponse.error(
        'Sistem belum siap melayani traffic',
        ApiResponse.errorCode.SERVICE_UNAVAILABLE,
        {
          status: 'not_ready',
          database: dbHealth,
          redis: redisHealth,
          timestamp: new Date().toISOString(),
        },
      );
    }

    return ApiResponse.success(
      {
        status: 'ready',
        database: dbHealth,
        redis: redisHealth,
        uptime: process.uptime(),
        timestamp: new Date().toISOString(),
      },
      'Seluruh dependensi sistem siap melayani traffic',
    );
  })

  // Mount JWKS Route at root (OIDC Standard)
  .use(jwksRoute)

  // Mount Tactical PTT Walkie-Talkie WebSocket Route
  .use(pttWsRoute)

  // Mount Feature Routes (v1)
  .group(AppConstants.apiVersion(), (v1) =>
    v1
      .use(authRoutes)
      .use(groupRoutes)
      .use(guestRoutes)
      .use(operationalRoutes)
      .use(attendanceRoutes)
      .use(informationRoutes)
      .use(incidentRoutes),
  );

export type App = typeof app;
