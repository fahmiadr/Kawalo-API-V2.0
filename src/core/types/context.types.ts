import type { AuthenticatedUserPayload } from './jwt-payload.types';

/**
 * Representasi setter status HTTP dan headers response.
 */
export interface HttpContextSet {
  status?: number | string;
  headers?: Record<string, string | number>;
}

/**
 * Konteks dasar untuk seluruh request handler (publik / unauthenticated).
 */
export interface BaseHandlerContext {
  set: HttpContextSet;
}

/**
 * Konteks request handler yang memerlukan otentikasi (membawa data sesi currentUser yang terverifikasi).
 */
export interface AuthenticatedHandlerContext extends BaseHandlerContext {
  currentUser: AuthenticatedUserPayload;
}
