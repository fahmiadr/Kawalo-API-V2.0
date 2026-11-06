import { EnvConfig } from '@core/config/env.config';
import { ApiResponse } from '@core/utils/api-response';
import { logger } from '@core/utils/logger.service';
import { Elysia } from 'elysia';

export const errorHandlerMiddleware = new Elysia({ name: 'error-handler' }).onError(
  { as: 'global' },
  ({ code, error, request, set, status }) => {
    const isProd = EnvConfig.isProduction;
    const pathname = new URL(request.url).pathname;

    const customReq = request as unknown as { _requestId?: string };
    const requestId = customReq._requestId || request.headers.get('x-request-id');
    if (requestId) {
      set.headers['x-request-id'] = requestId;
    }
    set.headers['x-content-type-options'] = 'nosniff';
    set.headers['x-frame-options'] = 'DENY';

    // 1. Validation Errors (TypeBox schema validation)
    if (code === 'VALIDATION') {
      const valError = error as {
        all?: Array<{ path: string; summary?: string; message?: string }>;
        message?: string;
      };
      let detailMsg = '';
      if (Array.isArray(valError.all) && valError.all.length > 0) {
        detailMsg = valError.all
          .map((item) => `${item.path}: ${item.summary || item.message || 'invalid'}`)
          .join(', ');
      } else {
        detailMsg = error instanceof Error ? error.message : String(error);
      }

      logger.warn(`[Validation Error] ${request.method} ${pathname} -> ${detailMsg}`);

      return status(
        'Unprocessable Content',
        ApiResponse.error(
          'Format data masukan tidak valid',
          ApiResponse.errorCode.VALIDATION_ERROR,
          isProd ? undefined : detailMsg,
        ),
      );
    }

    // 2. Parse Errors (Malformed JSON body)
    if (code === 'PARSE') {
      logger.warn(`[Parse Error] ${request.method} ${pathname} -> Malformed JSON body`);
      return status(
        'Bad Request',
        ApiResponse.error('Format data JSON tidak valid', ApiResponse.errorCode.BAD_REQUEST),
      );
    }

    // 3. Database Errors (Prisma ORM & SQL errors)
    const errObj = error as { name?: string; code?: string; message?: string };
    const isPrismaError =
      errObj?.name === 'PrismaClientKnownRequestError' ||
      errObj?.name === 'PrismaClientValidationError' ||
      errObj?.name === 'PrismaClientInitializationError' ||
      errObj?.name === 'PrismaClientRustPanicError' ||
      errObj?.name === 'PrismaClientUnknownRequestError' ||
      (typeof errObj?.code === 'string' && /^P\d{4}$/.test(errObj.code));

    if (isPrismaError) {
      logger.error(
        { err: error, code: errObj?.code },
        `[Database Error] ${request.method} ${pathname} -> ${errObj.message || 'Prisma error'}`,
      );

      // In production, NEVER leak internal database details or raw error messages to the client
      if (isProd) {
        return status(
          'Internal Server Error',
          ApiResponse.error(
            'Terjadi kesalahan pada sistem saat memproses data',
            ApiResponse.errorCode.INTERNAL_SERVER_ERROR,
          ),
        );
      }

      // In development / staging, provide debug details for local troubleshooting
      return status(
        'Internal Server Error',
        ApiResponse.error(
          'Kesalahan operasi basis data',
          ApiResponse.errorCode.DATABASE_ERROR,
          errObj.message,
        ),
      );
    }

    // 4. Not Found Errors
    if (code === 'NOT_FOUND') {
      logger.warn(`[Not Found] ${request.method} ${pathname}`);
      return status(
        'Not Found',
        ApiResponse.error(
          'Endpoint atau rute tidak ditemukan',
          ApiResponse.errorCode.RESOURCE_NOT_FOUND,
        ),
      );
    }

    // 5. Custom / Internal Server Errors
    logger.error(
      { err: error },
      `[Internal Error] ${request.method} ${pathname} -> ${error instanceof Error ? error.message : String(error)}`,
    );
    return status(
      'Internal Server Error',
      ApiResponse.error(
        'Terjadi kesalahan pada server internal',
        ApiResponse.errorCode.INTERNAL_SERVER_ERROR,
        isProd ? undefined : error instanceof Error ? error.stack : String(error),
      ),
    );
  },
);
