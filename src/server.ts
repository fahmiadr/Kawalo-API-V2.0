import { EnvConfig } from '@core/config/env.config';
import { AppConstants } from '@core/constants/app.constants';
import { PrismaService } from '@core/database/prisma.service';
import { RedisService } from '@core/database/redis.service';
import { RsaKeyManager } from '@core/security/rsa-key.manager';
import { logger } from '@core/utils/logger.service';
import { app } from '@/app';

async function bootstrap() {
  console.log('\n======================================================');
  console.log(`[BOOT] Memulai Server ${AppConstants.APP_NAME} (${EnvConfig.NODE_ENV})`);
  console.log('======================================================');

  // 1. Inisialisasi Kriptografi Kunci RS256 & Aktifkan Live Hot-Reload Watcher
  await RsaKeyManager.init();
  RsaKeyManager.enableFileWatcher();

  // 2. Eager Connection Handshake: Database Connection Pool (MariaDB) with Startup Retry
  await PrismaService.connect();

  // 3. Eager Connection Handshake: Redis Cache & Session with Startup Retry
  await RedisService.ping();

  // 4. Jalankan Server HTTP via Node.js
  const port = EnvConfig.PORT;
  app.listen(port, (info) => {
    const serverInfo = info as { hostname?: string; port?: number } | undefined;
    const host = serverInfo?.hostname || 'localhost';
    const p = serverInfo?.port || port;
    logger.info(`[Server] Server aktif di: http://${host}:${p}`);
    logger.info(`[JWKS] OIDC Endpoint: http://${host}:${p}/.well-known/jwks.json`);
    logger.info(`[Health] Liveness: http://${host}:${p}/health`);
    logger.info(`[Health] Readiness: http://${host}:${p}/health/ready`);
  });

  // 5. Graceful Shutdown
  const shutdown = async (signal: string) => {
    logger.info(`[Server] Menerima sinyal ${signal}. Memulai pematian bertahap...`);
    try {
      RsaKeyManager.closeFileWatcher();
      await PrismaService.disconnect();
      await RedisService.disconnect();
      logger.info('[Server] Koneksi database & Redis berhasil ditutup.');
      process.exit(0);
    } catch (err) {
      logger.error({ err }, '[Server] Error saat shutdown');
      process.exit(1);
    }
  };

  process.on('SIGINT', () => shutdown('SIGINT'));
  process.on('SIGTERM', () => shutdown('SIGTERM'));
}

bootstrap().catch((err) => {
  logger.error({ err }, 'Fatal Error saat bootstrap server');
  process.exit(1);
});
