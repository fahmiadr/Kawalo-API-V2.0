/**
 * Peta Status HTTP Terstandarisasi (Framework-Agnostic)
 * Sesuai RFC 9110 & spesifikasi semantik HTTP tanpa dependensi external library/framework.
 */
export const HttpStatusMap = {
  OK: 200,
  Created: 201,
  Accepted: 202,
  'No Content': 204,
  'Bad Request': 400,
  Unauthorized: 401,
  Forbidden: 403,
  'Not Found': 404,
  'Method Not Allowed': 405,
  Conflict: 409,
  'Unprocessable Content': 422,
  'Too Many Requests': 429,
  'Internal Server Error': 500,
  'Not Implemented': 501,
  'Bad Gateway': 502,
  'Service Unavailable': 503,
  'Gateway Timeout': 504,
} as const;

export type HttpStatusName = keyof typeof HttpStatusMap;

export const InvertedHttpStatusMap: Record<number, HttpStatusName> = {
  200: 'OK',
  201: 'Created',
  202: 'Accepted',
  204: 'No Content',
  400: 'Bad Request',
  401: 'Unauthorized',
  403: 'Forbidden',
  404: 'Not Found',
  405: 'Method Not Allowed',
  409: 'Conflict',
  422: 'Unprocessable Content',
  429: 'Too Many Requests',
  500: 'Internal Server Error',
  501: 'Not Implemented',
  502: 'Bad Gateway',
  503: 'Service Unavailable',
  504: 'Gateway Timeout',
};
