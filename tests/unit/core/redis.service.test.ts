import { RedisService } from '@core/database/redis.service';
import { logger } from '@core/utils/logger.service';
import { describe, expect, it, vi } from 'vitest';

describe('RedisService Tests', () => {
  it('harus mencatat log info ketika berhasil terhubung kembali setelah reconnecting', () => {
    const client = RedisService.getInstance();

    const warnSpy = vi.spyOn(logger, 'warn');
    const infoSpy = vi.spyOn(logger, 'info');

    // 1. Simulasikan event reconnecting
    client.emit('reconnecting');
    expect(warnSpy).toHaveBeenCalledWith('[Redis] Menyambung ulang ke Redis server...');

    // 2. Simulasikan event ready setelah reconnecting
    client.emit('ready');
    expect(infoSpy).toHaveBeenCalledWith('[Redis] Berhasil terhubung kembali ke Redis server.');

    // 3. Event ready selanjutnya tanpa reconnecting tidak boleh mencatat log reconnecting berhasil lagi
    infoSpy.mockClear();
    client.emit('ready');
    expect(infoSpy).not.toHaveBeenCalledWith('[Redis] Berhasil terhubung kembali ke Redis server.');

    warnSpy.mockRestore();
    infoSpy.mockRestore();
  });

  it('harus mengembalikan status healthy saat healthCheck berhasil', async () => {
    const client = RedisService.getInstance();
    vi.spyOn(client, 'ping').mockResolvedValueOnce('PONG');

    const result = await RedisService.healthCheck();
    expect(result.status).toBe('healthy');
    expect(result.latencyMs).toBeDefined();
  });

  it('harus mengembalikan status unhealthy saat healthCheck gagal', async () => {
    const client = RedisService.getInstance();
    vi.spyOn(client, 'ping').mockRejectedValueOnce(new Error('Connection lost'));

    const result = await RedisService.healthCheck();
    expect(result.status).toBe('unhealthy');
    expect(result.error).toBe('Connection lost');
  });

  it('harus memanggil quit saat disconnect', async () => {
    const client = RedisService.getInstance();
    const quitSpy = vi.spyOn(client, 'quit').mockResolvedValueOnce('OK');

    await RedisService.disconnect();
    expect(quitSpy).toHaveBeenCalled();
  });
});
