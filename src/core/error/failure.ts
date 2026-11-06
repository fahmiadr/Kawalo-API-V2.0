import { type ErrorCode, ErrorCodes } from '@core/constants/error.codes';
import { HttpStatusMap, type HttpStatusName, InvertedHttpStatusMap } from './http-status';

export type { HttpStatusName };

/**
 * Nilai status kode error (4xx dan 5xx) yang didukung oleh arsitektur sistem.
 */
export type AppErrorStatusCode =
  | 400
  | 401
  | 403
  | 404
  | 405
  | 409
  | 422
  | 429
  | 500
  | 501
  | 502
  | 503
  | 504;

export abstract class Failure {
  public readonly status: HttpStatusName;

  constructor(
    public readonly message: string,
    status: HttpStatusName | number = 'Internal Server Error',
    public readonly errorCode: ErrorCode = ErrorCodes.INTERNAL_SERVER_ERROR,
    public readonly details?: unknown,
    public readonly retryAfterSeconds?: number,
  ) {
    if (typeof status === 'number') {
      this.status = InvertedHttpStatusMap[status] ?? 'Internal Server Error';
    } else {
      this.status = status;
    }
  }

  /**
   * Mengembalikan kode status HTTP numerik terstandarisasi (e.g. 500, 401, 404).
   */
  public get statusCode(): AppErrorStatusCode {
    return (HttpStatusMap[this.status] ?? 500) as AppErrorStatusCode;
  }

  /**
   * Template Method (GoF Behavioral Pattern):
   * Menyediakan representasi terstandarisasi untuk logger, audit trail, dan serialisasi JSON.
   */
  public toJSON() {
    return {
      message: this.message,
      status: this.status,
      statusCode: this.statusCode,
      errorCode: this.errorCode,
      details: this.details,
      retryAfterSeconds: this.retryAfterSeconds,
    };
  }

  // =========================================================================
  // GoF Factory Methods (Creational Pattern)
  // =========================================================================
  public static server(message?: string, details?: unknown): ServerFailure {
    return new ServerFailure(message, details);
  }

  public static database(message?: string, details?: unknown): DatabaseFailure {
    return new DatabaseFailure(message, details);
  }

  public static auth(
    message?: string,
    errorCode?: ErrorCode,
    status?: HttpStatusName | number,
    retryAfterSeconds?: number,
    details?: unknown,
  ): AuthFailure {
    return new AuthFailure(message, errorCode, status, retryAfterSeconds, details);
  }

  public static locked(remainingSeconds: number, message?: string): AuthFailure {
    const lockMinutes = Math.ceil(remainingSeconds / 60);
    return new AuthFailure(
      message ??
        `Akun Anda dikunci sementara karena beberapa kali kegagalan masuk. Silakan coba kembali dalam ${lockMinutes} menit.`,
      ErrorCodes.AUTH_ACCOUNT_LOCKED,
      'Too Many Requests',
      remainingSeconds,
    );
  }

  public static rateLimit(retryAfterSeconds?: number, message?: string): RateLimitFailure {
    return new RateLimitFailure(message, retryAfterSeconds);
  }

  public static validation(message?: string, details?: unknown): ValidationFailure {
    return new ValidationFailure(message, details);
  }

  public static notFound(message?: string): NotFoundFailure {
    return new NotFoundFailure(message);
  }

  public static forbidden(message?: string): ForbiddenFailure {
    return new ForbiddenFailure(message);
  }
}

export class ServerFailure extends Failure {
  constructor(message = 'Terjadi kesalahan pada server internal', details?: unknown) {
    super(message, 'Internal Server Error', ErrorCodes.INTERNAL_SERVER_ERROR, details);
  }
}

export class DatabaseFailure extends Failure {
  constructor(message = 'Terjadi kesalahan saat mengakses database', details?: unknown) {
    super(message, 'Internal Server Error', ErrorCodes.DATABASE_ERROR, details);
  }
}

export class AuthFailure extends Failure {
  constructor(
    message = 'Email atau password yang Anda masukkan salah',
    errorCode: ErrorCode = ErrorCodes.AUTH_INVALID_CREDENTIALS,
    status: HttpStatusName | number = 'Unauthorized',
    retryAfterSeconds?: number,
    details?: unknown,
  ) {
    super(message, status, errorCode, details, retryAfterSeconds);
  }
}

export class ValidationFailure extends Failure {
  constructor(message = 'Data yang dikirim tidak valid', details?: unknown) {
    super(message, 'Bad Request', ErrorCodes.VALIDATION_ERROR, details);
  }
}

export class NotFoundFailure extends Failure {
  constructor(message = 'Data atau sumber daya tidak ditemukan') {
    super(message, 'Not Found', ErrorCodes.RESOURCE_NOT_FOUND);
  }
}

export class ForbiddenFailure extends Failure {
  constructor(message = 'Anda tidak memiliki hak akses untuk tindakan ini') {
    super(message, 'Forbidden', ErrorCodes.AUTH_FORBIDDEN);
  }
}

export class RateLimitFailure extends Failure {
  constructor(
    message = 'Terlalu banyak permintaan. Silakan coba beberapa saat lagi.',
    retryAfterSeconds?: number,
  ) {
    super(
      message,
      'Too Many Requests',
      ErrorCodes.RATE_LIMIT_EXCEEDED,
      undefined,
      retryAfterSeconds,
    );
  }
}
