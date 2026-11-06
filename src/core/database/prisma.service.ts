import { EnvConfig } from '@core/config/env.config';
import { logger } from '@core/utils/logger.service';
import { type Prisma, PrismaClient } from '@generated/prisma/client';
import { PrismaMariaDb } from '@prisma/adapter-mariadb';

// Safe-guard Date.prototype.toISOString for legacy zero-dates (0000-00-00 00:00:00) in MariaDB adapter.
// Idempotent monkey-patching: mencegah multiple wrapping saat hot-reload atau test suite execution.
const DATE_PATCH_KEY = Symbol.for('__kawalo_date_iso_patched__');
const proto = Date.prototype as unknown as Record<symbol, boolean>;
if (!proto[DATE_PATCH_KEY]) {
  const originalToISOString = Date.prototype.toISOString;
  Date.prototype.toISOString = function () {
    if (Number.isNaN(this.getTime())) {
      return '1970-01-01T00:00:00.000Z';
    }
    return originalToISOString.call(this);
  };
  proto[DATE_PATCH_KEY] = true;
}

/**
 * Menormalisasi DATABASE_URL untuk driver mariadb:
 * 1. Protokol URL: Menyeragamkan skema protokol 'mysql:' menjadi 'mariadb:'.
 * 2. Pool Parameter: Mengonversi parameter snake_case Prisma klasik (connection_limit, connect_timeout, dll.)
 *    menjadi camelCase format yang dikenali oleh driver MariaDB (connectionLimit, connectTimeout).
 * 3. Driver Timeouts: Menyuntikkan default connectTimeout (5000ms) dan socketTimeout (30000ms)
 *    jika belum dispesifikasikan dalam URL untuk mencegah connection hang atau timeout prematur di lingkungan cloud.
 */
export function normalizeDatabaseUrl(rawUrl: string): string {
  let urlStr = rawUrl.replace(/^mysql:/, 'mariadb:');

  // Normalisasi query parameter snake_case ke camelCase
  urlStr = urlStr
    .replace(/([?&])connection_limit=/g, '$1connectionLimit=')
    .replace(/([?&])connect_timeout=/g, '$1connectTimeout=')
    .replace(/([?&])socket_timeout=/g, '$1socketTimeout=')
    .replace(/([?&])acquire_timeout=/g, '$1acquireTimeout=');

  try {
    const parsed = new URL(urlStr);
    // Default connectTimeout (5 detik alih-alih 1 detik bawaan driver)
    if (!parsed.searchParams.has('connectTimeout')) {
      parsed.searchParams.set('connectTimeout', '5000');
    }
    // Default socketTimeout (30 detik alih-alih 0 / infinite bawaan driver)
    if (!parsed.searchParams.has('socketTimeout')) {
      parsed.searchParams.set('socketTimeout', '30000');
    }
    return parsed.toString();
  } catch {
    return urlStr;
  }
}

class PrismaService {
  private static instance: PrismaClient | undefined;

  public static getInstance(): PrismaClient {
    if (!PrismaService.instance) {
      const url = normalizeDatabaseUrl(EnvConfig.DATABASE_URL);

      // Aspek 1 & 3: Driver Adapter & Execution Protocol (text vs binary)
      const adapter = new PrismaMariaDb(url, {
        useTextProtocol: EnvConfig.DATABASE_USE_TEXT_PROTOCOL,
        onConnectionError: (err) => {
          logger.error({ err }, '[Prisma:MariaDB] Error pada koneksi pool database');
        },
      });

      const client = new PrismaClient({
        adapter,
        log: [
          { level: 'query', emit: 'event' },
          { level: 'error', emit: 'event' },
          { level: 'warn', emit: 'event' },
        ],
      }) as unknown as PrismaClient<'query' | 'warn' | 'error'>;

      // Filter and log slow queries (>= 100ms) dengan masking parameter sensitif di production
      client.$on('query', (event: Prisma.QueryEvent) => {
        if (event.duration >= 100) {
          const params = EnvConfig.isProduction ? '[REDACTED]' : event.params;
          logger.warn(
            `[Prisma:SlowQuery] [${event.duration}ms] ${event.query} (params: ${params})`,
          );
        }
      });

      client.$on('warn', (event: Prisma.LogEvent) => {
        logger.warn(`[Prisma Engine Warning] ${event.message}`);
      });

      client.$on('error', (event: Prisma.LogEvent) => {
        logger.error(`[Prisma Engine Error] ${event.message}`);
      });

      PrismaService.instance = client as unknown as PrismaClient;
    }
    return PrismaService.instance;
  }

  /**
   * Eager connection handshake dengan probe query (SELECT 1) dan startup retry.
   * Mencegah false positive dengan memverifikasi koneksi fisik TCP sebelum server membuka port.
   */
  public static async connect(retries = 5, delayMs = 2000): Promise<void> {
    const client = PrismaService.getInstance();
    for (let attempt = 1; attempt <= retries; attempt++) {
      try {
        const start = performance.now();
        await client.$connect();
        // Eager probe query: memverifikasi soket TCP & autentikasi riil ke server MariaDB
        await client.$queryRaw`SELECT 1`;
        const latency = Math.round(performance.now() - start);
        logger.info(
          `[DB:Pool] Database Connection Pool (MariaDB) aktif & siap melayani query (${latency}ms)`,
        );
        return;
      } catch (err: unknown) {
        const errorMsg = err instanceof Error ? err.message : String(err);
        if (attempt === retries) {
          logger.error(
            { err },
            `[DB:Pool] Gagal terhubung ke MariaDB setelah ${retries} percobaan. Error: ${errorMsg}`,
          );
          throw new Error(
            `[DB:Pool] Gagal terhubung ke MariaDB setelah ${retries} percobaan: ${errorMsg}`,
          );
        }
        logger.warn(
          `[DB:Pool] Percobaan koneksi ${attempt}/${retries} gagal (${errorMsg}). Mencoba lagi dalam ${delayMs / 1000} detik...`,
        );
        await new Promise((resolve) => setTimeout(resolve, delayMs));
      }
    }
  }

  /**
   * Pemeriksaan kesehatan database untuk endpoint readiness probe (/health/ready).
   * Dilengkapi timeout guardrail (default 3000ms) untuk mencegah request menggantung saat socket freeze.
   */
  public static async healthCheck(timeoutMs = 3000): Promise<{
    status: 'healthy' | 'unhealthy';
    latencyMs?: number;
    error?: string;
  }> {
    let timer: NodeJS.Timeout | undefined;
    try {
      const client = PrismaService.getInstance();
      const start = performance.now();

      const timeoutPromise = new Promise<never>((_, reject) => {
        timer = setTimeout(
          () => reject(new Error(`Database health check timed out (> ${timeoutMs}ms)`)),
          timeoutMs,
        );
      });

      await Promise.race([client.$queryRaw`SELECT 1`, timeoutPromise]);

      return {
        status: 'healthy',
        latencyMs: Math.round(performance.now() - start),
      };
    } catch (err: unknown) {
      return {
        status: 'unhealthy',
        error: err instanceof Error ? err.message : String(err),
      };
    } finally {
      if (timer) clearTimeout(timer);
    }
  }

  public static async disconnect(): Promise<void> {
    if (PrismaService.instance) {
      const client = PrismaService.instance;
      PrismaService.instance = undefined;
      await client.$disconnect();
    }
  }
}

export const prisma = PrismaService.getInstance();
export { PrismaService };
