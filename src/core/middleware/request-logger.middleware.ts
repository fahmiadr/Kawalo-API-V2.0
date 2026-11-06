import { randomUUID } from 'node:crypto';
import { cleanInteger } from '@core/utils/clean-data.util';
import { resolveClientIp } from '@core/utils/client-ip.util';
import { logger } from '@core/utils/logger.service';
import { Elysia, StatusMap } from 'elysia';

export const requestLoggerMiddleware = new Elysia({ name: 'request-logger' })
  .use(logger.into({ autoLogging: false }))
  .onRequest(({ request }) => {
    const customReq = request as unknown as { _startTime?: number; _requestId?: string };
    customReq._startTime = performance.now();

    let requestId = request.headers.get('x-request-id');
    if (!requestId) {
      requestId = randomUUID();
      request.headers.set('x-request-id', requestId);
    }
    customReq._requestId = requestId;
  })
  .onBeforeHandle({ as: 'global' }, ({ request, set }) => {
    const customReq = request as unknown as { _requestId?: string };
    const requestId = customReq._requestId || request.headers.get('x-request-id');
    if (requestId) {
      set.headers['x-request-id'] = requestId;
    }
  })
  .onAfterResponse({ as: 'global' }, ({ request, set }) => {
    const url = new URL(request.url);
    if (url.pathname === '/health') {
      return;
    }

    const customReq = request as unknown as { _startTime?: number; _requestId?: string };
    const startTime = customReq._startTime || performance.now();
    const duration = (performance.now() - startTime).toFixed(2);
    const requestId = customReq._requestId || request.headers.get('x-request-id') || '-';
    const rawStatus = set.status;
    let status: number;
    if (typeof rawStatus === 'number') {
      status = rawStatus;
    } else if (typeof rawStatus === 'string' && rawStatus in StatusMap) {
      status = StatusMap[rawStatus as keyof typeof StatusMap];
    } else {
      status = cleanInteger(rawStatus) ?? 200;
    }

    const clientIp = resolveClientIp(request);
    const message = `${request.method} ${url.pathname} ${status} (${duration}ms) [${requestId}] [${clientIp}]`;

    if (status >= 500) {
      logger.error({ reqMsg: message });
    } else if (status >= 400) {
      logger.warn({ reqMsg: message });
    } else {
      logger.info({ reqMsg: message });
    }
  });
