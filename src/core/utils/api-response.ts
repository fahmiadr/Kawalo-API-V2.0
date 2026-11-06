import { EnvConfig } from '@core/config/env.config';
import { type ErrorCode, ErrorCodes } from '@core/constants/error.codes';
import type { Failure } from '@core/error/failure';
import { type HttpStatusName, InvertedHttpStatusMap } from '@core/error/http-status';

import { type TSchema, t } from 'elysia';

export type SuccessApiResponse<T = unknown> = {
  success: true;
  message: string;
  data: T;
};

export type ErrorApiResponse = {
  success: false;
  message: string;
  error_code?: ErrorCode;
  details?: unknown;
};

export type StandardApiResponse<T = unknown> = SuccessApiResponse<T> | ErrorApiResponse;

export const ApiResponseSuccessSchema = <T extends TSchema>(dataSchema: T) =>
  t.Object({
    success: t.Literal(true),
    message: t.String(),
    data: dataSchema,
  });

export const ApiResponseErrorSchema = t.Object({
  success: t.Literal(false),
  message: t.String(),
  error_code: t.Optional(t.Enum(ErrorCodes, { description: 'Kode error terstandarisasi sistem' })),
  details: t.Optional(t.Unknown()),
});

/**
 * Pemetaan skema kontrak error terstandarisasi untuk seluruh rute API
 */
export const StandardErrorResponseSchemas = {
  400: ApiResponseErrorSchema,
  401: ApiResponseErrorSchema,
  403: ApiResponseErrorSchema,
  404: ApiResponseErrorSchema,
  429: ApiResponseErrorSchema,
  500: ApiResponseErrorSchema,
  503: ApiResponseErrorSchema,
};

/**
 * Kontrak skema respons standar untuk status 200 OK + seluruh status error
 */
export const ApiResponseSchema = <T extends TSchema>(dataSchema: T) => ({
  200: ApiResponseSuccessSchema(dataSchema),
  ...StandardErrorResponseSchemas,
});

/**
 * Kontrak skema respons standar untuk status 201 Created + seluruh status error
 */
export const ApiCreatedResponseSchema = <T extends TSchema>(dataSchema: T) => ({
  201: ApiResponseSuccessSchema(dataSchema),
  ...StandardErrorResponseSchemas,
});

export type { HttpStatusName };
export const HttpStatus = InvertedHttpStatusMap;

export class ApiResponse {
  /**
   * Error code terstandarisasi sistem (autocomplete: ApiResponse.errorCode.*)
   */
  public static readonly errorCode = ErrorCodes;

  /**
   * Mapping HTTP Status Code ke nama semantik terstandarisasi (e.g. ApiResponse.status[401] => 'Unauthorized')
   */
  public static readonly status = InvertedHttpStatusMap;

  public static success<T>(data: T, message = 'Berhasil'): SuccessApiResponse<T> {
    return {
      success: true,
      message,
      data,
    };
  }

  public static error(message: string, errorCode: ErrorCode, details?: unknown): ErrorApiResponse {
    return {
      success: false,
      message,
      error_code: errorCode,
      details: EnvConfig.isProduction ? undefined : details,
    };
  }

  public static fromFailure(failure: Failure): ErrorApiResponse {
    return ApiResponse.error(failure.message, failure.errorCode, failure.details);
  }
}
