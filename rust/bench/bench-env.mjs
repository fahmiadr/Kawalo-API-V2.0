// Single source of truth for the environment both servers run under, so the comparison is apples to apples.
// Every key is set explicitly: the TS server would otherwise fall back to .env.prod / .env via dotenv.
export const DATABASE_URL = 'mysql://root:@127.0.0.1:3392/kawalo_bench?connection_limit=10';
export const REDIS_URL = 'redis://127.0.0.1:6393';

export function serverEnv(port) {
  return {
    NODE_ENV: 'production',
    PORT: String(port),
    LOG_LEVEL: 'warn',
    DATABASE_URL,
    DATABASE_USE_TEXT_PROTOCOL: 'false',
    REDIS_URL,
    JWT_EXPIRES_IN: '900',
    REFRESH_TOKEN_EXPIRES_IN: '1209600',
    CORS_ORIGIN: '*',
    // The app-level limiter would otherwise reject the load generator (single IP) after 100 requests.
    RATE_LIMIT_MAX: '1000000000',
    RATE_LIMIT_DURATION: '60000',
    LOGIN_RATE_LIMIT_IP_MAX: '1000000000',
    LOGIN_MAX_FAILED_ATTEMPTS: '1000000000',
    TRUSTED_PROXY_CIDRS: '127.0.0.1/32,::1/128',
  };
}
