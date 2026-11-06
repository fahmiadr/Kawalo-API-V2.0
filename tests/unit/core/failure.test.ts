import {
  AuthFailure,
  DatabaseFailure,
  Failure,
  ForbiddenFailure,
  NotFoundFailure,
  RateLimitFailure,
  ServerFailure,
  ValidationFailure,
} from '@core/error/failure';
import { HttpStatusMap, InvertedHttpStatusMap } from '@core/error/http-status';
import { describe, expect, it } from 'vitest';

describe('Failure & HttpStatus Unit Tests', () => {
  it('harus memetakan status code numerik dan nama status HTTP dengan benar', () => {
    expect(HttpStatusMap['OK']).toBe(200);
    expect(HttpStatusMap['Created']).toBe(201);
    expect(HttpStatusMap['Bad Request']).toBe(400);
    expect(HttpStatusMap['Unauthorized']).toBe(401);
    expect(HttpStatusMap['Forbidden']).toBe(403);
    expect(HttpStatusMap['Not Found']).toBe(404);
    expect(HttpStatusMap['Internal Server Error']).toBe(500);

    expect(InvertedHttpStatusMap[200]).toBe('OK');
    expect(InvertedHttpStatusMap[401]).toBe('Unauthorized');
    expect(InvertedHttpStatusMap[404]).toBe('Not Found');
    expect(InvertedHttpStatusMap[500]).toBe('Internal Server Error');
  });

  it('ServerFailure harus memiliki status Internal Server Error dan statusCode 500', () => {
    const failure = new ServerFailure('Database timeout');
    expect(failure.status).toBe('Internal Server Error');
    expect(failure.statusCode).toBe(500);
    expect(failure.message).toBe('Database timeout');
  });

  it('ValidationFailure harus memiliki status Bad Request dan statusCode 400', () => {
    const failure = new ValidationFailure('Email tidak valid', { field: 'email' });
    expect(failure.status).toBe('Bad Request');
    expect(failure.statusCode).toBe(400);
    expect(failure.details).toEqual({ field: 'email' });
  });

  it('AuthFailure default harus memiliki status Unauthorized dan statusCode 401', () => {
    const failure = new AuthFailure();
    expect(failure.status).toBe('Unauthorized');
    expect(failure.statusCode).toBe(401);
  });

  it('NotFoundFailure harus memiliki status Not Found dan statusCode 404', () => {
    const failure = new NotFoundFailure();
    expect(failure.status).toBe('Not Found');
    expect(failure.statusCode).toBe(404);
  });

  it('ForbiddenFailure harus memiliki status Forbidden dan statusCode 403', () => {
    const failure = new ForbiddenFailure();
    expect(failure.status).toBe('Forbidden');
    expect(failure.statusCode).toBe(403);
  });

  it('RateLimitFailure harus memiliki status Too Many Requests dan statusCode 429', () => {
    const failure = new RateLimitFailure();
    expect(failure.status).toBe('Too Many Requests');
    expect(failure.statusCode).toBe(429);
  });

  it('Failure subclass custom dengan status numerik harus memetakan ke nama status yang tepat', () => {
    class CustomFailure extends Failure {}
    const failure = new CustomFailure('Custom error', 404);
    expect(failure.status).toBe('Not Found');
    expect(failure.statusCode).toBe(404);
  });

  it('GoF Factory Methods harus menghasilkan instance Failure yang tepat', () => {
    const serverFail = Failure.server('Server timeout', { code: 'ETIMEDOUT' });
    expect(serverFail).toBeInstanceOf(ServerFailure);
    expect(serverFail.statusCode).toBe(500);

    const dbFail = Failure.database('DB connection failed');
    expect(dbFail).toBeInstanceOf(DatabaseFailure);
    expect(dbFail.statusCode).toBe(500);

    const authFail = Failure.auth('Invalid creds');
    expect(authFail).toBeInstanceOf(AuthFailure);
    expect(authFail.statusCode).toBe(401);

    const lockedFail = Failure.locked(900);
    expect(lockedFail).toBeInstanceOf(AuthFailure);
    expect(lockedFail.statusCode).toBe(429);
    expect(lockedFail.retryAfterSeconds).toBe(900);
    expect(lockedFail.message).toContain('15 menit');

    const rateLimitFail = Failure.rateLimit(60, 'Rate limit hit');
    expect(rateLimitFail).toBeInstanceOf(RateLimitFailure);
    expect(rateLimitFail.statusCode).toBe(429);
    expect(rateLimitFail.retryAfterSeconds).toBe(60);

    const valFail = Failure.validation('Invalid format');
    expect(valFail).toBeInstanceOf(ValidationFailure);
    expect(valFail.statusCode).toBe(400);

    const notFoundFail = Failure.notFound('Data not found');
    expect(notFoundFail).toBeInstanceOf(NotFoundFailure);
    expect(notFoundFail.statusCode).toBe(404);

    const forbiddenFail = Failure.forbidden('Access denied');
    expect(forbiddenFail).toBeInstanceOf(ForbiddenFailure);
    expect(forbiddenFail.statusCode).toBe(403);
  });

  it('Template Method toJSON harus mengembalikan objek serialisasi lengkap', () => {
    const failure = Failure.locked(600, 'Akun dikunci');
    const json = failure.toJSON();

    expect(json).toEqual({
      message: 'Akun dikunci',
      status: 'Too Many Requests',
      statusCode: 429,
      errorCode: 'AUTH_ACCOUNT_LOCKED',
      details: undefined,
      retryAfterSeconds: 600,
    });
  });
});
