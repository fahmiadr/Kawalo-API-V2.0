import { EnvConfig } from '@core/config/env.config';
import { logger } from '@core/utils/logger.service';
import Redis, { type RedisOptions } from 'ioredis';

class RedisService {
  private static instance: Redis;
  private static isReconnecting = false;

  public static getInstance(): Redis {
    if (!RedisService.instance) {
      const redisOptions: RedisOptions = {
        maxRetriesPerRequest: 3,
        connectTimeout: 5000,
        commandTimeout: 3000,
        enableReadyCheck: true,
        retryStrategy(times) {
          return Math.min(times * 100, 2000);
        },
        reconnectOnError(err) {
          const targetError = 'READONLY';
          if (err.message.includes(targetError)) {
            return true;
          }
          return false;
        },
      };

      RedisService.instance = new Redis(EnvConfig.REDIS_URL, redisOptions);

      RedisService.instance.on('error', (err) => {
        logger.error({ err }, `[Redis] Connection error: ${err.message}`);
      });

      RedisService.instance.on('reconnecting', () => {
        RedisService.isReconnecting = true;
        logger.warn('[Redis] Menyambung ulang ke Redis server...');
      });

      RedisService.instance.on('ready', () => {
        if (RedisService.isReconnecting) {
          logger.info('[Redis] Berhasil terhubung kembali ke Redis server.');
          RedisService.isReconnecting = false;
        }
      });
    }
    return RedisService.instance;
  }

  /**
   * Eager connection handshake dengan PING dan startup retry yang konsisten dengan database.
   */
  public static async ping(retries = 5, delayMs = 2000): Promise<string> {
    const client = RedisService.getInstance();
    for (let attempt = 1; attempt <= retries; attempt++) {
      try {
        const start = performance.now();
        const res = await client.ping();
        const latency = Math.round(performance.now() - start);
        logger.info(`[Redis] Tersambung & siap melayani cache/sesi (${latency}ms, ${res})`);
        return res;
      } catch (err: unknown) {
        const errorMsg = err instanceof Error ? err.message : String(err);
        if (attempt === retries) {
          logger.error(
            { err },
            `[Redis] Gagal terhubung ke Redis setelah ${retries} percobaan. Error: ${errorMsg}`,
          );
          throw new Error(
            `[Redis] Gagal terhubung ke Redis setelah ${retries} percobaan: ${errorMsg}`,
          );
        }
        logger.warn(
          `[Redis] Percobaan koneksi ${attempt}/${retries} gagal (${errorMsg}). Mencoba lagi dalam ${delayMs / 1000} detik...`,
        );
        await new Promise((resolve) => setTimeout(resolve, delayMs));
      }
    }
    throw new Error(`[Redis] Gagal terhubung ke Redis setelah ${retries} percobaan`);
  }

  /**
   * Pemeriksaan kesehatan Redis untuk endpoint readiness probe (/health/ready).
   */
  public static async healthCheck(): Promise<{
    status: 'healthy' | 'unhealthy';
    latencyMs?: number;
    error?: string;
  }> {
    try {
      const client = RedisService.getInstance();
      const start = performance.now();
      await client.ping();
      return {
        status: 'healthy',
        latencyMs: Math.round(performance.now() - start),
      };
    } catch (err: unknown) {
      return {
        status: 'unhealthy',
        error: err instanceof Error ? err.message : String(err),
      };
    }
  }

  public static async disconnect(): Promise<void> {
    if (RedisService.instance) {
      await RedisService.instance.quit();
    }
  }
}

export const redis = RedisService.getInstance();
export { RedisService };
