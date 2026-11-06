export type ApiVersion = `v${number}` | string | number;

export const AppConstants = {
  API_BASE_PATH: '/api',
  DEFAULT_API_VERSION: 'v1',

  /**
   * Helper dinamis untuk menghasilkan prefix rute API per versi (e.g. '/api/v1', '/api/v2', '/api/v3').
   * Mendukung input berupa string ('v1', 'v2', '2') maupun number (1, 2).
   */
  apiVersion(version?: ApiVersion): string {
    const target =
      version !== undefined && version !== null ? String(version).trim() : this.DEFAULT_API_VERSION;

    const cleanVersion = target.startsWith('v') ? target : `v${target}`;
    return `${this.API_BASE_PATH}/${cleanVersion}`;
  },

  APP_NAME: 'Kawalo Mobile API',
  DEFAULT_PAGE_SIZE: 20,
  MAX_PAGE_SIZE: 100,
  AUTH_TOKEN_TYPE: 'Bearer',
  JWT_ISSUER: 'kawalo_auth',
  JWT_AUDIENCE: 'kawalo_mobile',
};
