import { normalizeDatabaseUrl, PrismaService, prisma } from '@core/database/prisma.service';
import { describe, expect, it, vi } from 'vitest';

describe('PrismaService & Database Connection Unit Tests', () => {
  describe('normalizeDatabaseUrl', () => {
    it('harus mengubah skema mysql: menjadi mariadb: dan menyuntikkan timeout default', () => {
      const raw = 'mysql://root:secret@localhost:3306/kawalo';
      const normalized = normalizeDatabaseUrl(raw);

      expect(normalized).toContain('mariadb://root:secret@localhost:3306/kawalo');
      expect(normalized).toContain('connectTimeout=5000');
      expect(normalized).toContain('socketTimeout=30000');
    });

    it('harus menormalisasi parameter snake_case Prisma klasik ke camelCase MariaDB driver', () => {
      const raw =
        'mysql://app:pass@db:3306/kawalo?connection_limit=25&connect_timeout=8000&socket_timeout=45000&acquire_timeout=15000';
      const normalized = normalizeDatabaseUrl(raw);

      expect(normalized).toContain('mariadb://');
      expect(normalized).toContain('connectionLimit=25');
      expect(normalized).toContain('connectTimeout=8000');
      expect(normalized).toContain('socketTimeout=45000');
      expect(normalized).toContain('acquireTimeout=15000');
      expect(normalized).not.toContain('connection_limit=');
      expect(normalized).not.toContain('connect_timeout=');
    });

    it('harus mempertahankan timeout eksplisit dan tidak menimpa jika sudah ada di URL', () => {
      const raw = 'mariadb://root:@localhost:3306/kawalo?connectTimeout=10000&socketTimeout=60000';
      const normalized = normalizeDatabaseUrl(raw);

      expect(normalized).toContain('connectTimeout=10000');
      expect(normalized).toContain('socketTimeout=60000');
    });
  });

  describe('Date.prototype.toISOString Safe-guard', () => {
    it('harus mengembalikan 1970-01-01T00:00:00.000Z untuk Invalid Date (NaN)', () => {
      const invalidDate = new Date(Number.NaN);
      expect(Number.isNaN(invalidDate.getTime())).toBe(true);
      expect(invalidDate.toISOString()).toBe('1970-01-01T00:00:00.000Z');
    });

    it('harus memformat tanggal valid secara normal tanpa distorsi', () => {
      const validDate = new Date('2026-09-19T12:00:00.000Z');
      expect(validDate.toISOString()).toBe('2026-09-19T12:00:00.000Z');
    });
  });

  describe('PrismaService Lifecycle & Singleton', () => {
    it('harus mengembalikan instance PrismaClient yang sama (Singleton)', () => {
      const instance1 = PrismaService.getInstance();
      const instance2 = PrismaService.getInstance();

      expect(instance1).toBeDefined();
      expect(instance1).toBe(instance2);
      expect(prisma).toBeDefined();
    });

    it('healthCheck harus sukses dan mengembalikan status healthy dengan latencyMs', async () => {
      const health = await PrismaService.healthCheck(5000);

      expect(health.status).toBe('healthy');
      expect(typeof health.latencyMs).toBe('number');
      expect(health.latencyMs).toBeGreaterThanOrEqual(0);
    });

    it('healthCheck harus menangani timeout dan mengembalikan status unhealthy', async () => {
      const client = PrismaService.getInstance();
      vi.spyOn(client, '$queryRaw').mockImplementation(
        () =>
          new Promise((resolve) => setTimeout(resolve, 200)) as unknown as ReturnType<
            typeof client.$queryRaw
          >,
      );

      const health = await PrismaService.healthCheck(50); // timeout 50ms < 200ms

      expect(health.status).toBe('unhealthy');
      expect(health.error).toContain('Database health check timed out (> 50ms)');

      vi.restoreAllMocks();
    });

    it('healthCheck harus menangani query error dan mengembalikan status unhealthy', async () => {
      const client = PrismaService.getInstance();
      vi.spyOn(client, '$queryRaw').mockRejectedValueOnce(new Error('Connection terminated'));

      const health = await PrismaService.healthCheck(3000);

      expect(health.status).toBe('unhealthy');
      expect(health.error).toBe('Connection terminated');

      vi.restoreAllMocks();
    });

    it('disconnect harus memanggil $disconnect dan mereset instance singleton', async () => {
      const client = PrismaService.getInstance();
      const disconnectSpy = vi.spyOn(client, '$disconnect').mockResolvedValueOnce();

      await PrismaService.disconnect();

      expect(disconnectSpy).toHaveBeenCalledTimes(1);

      // Setelah disconnect, getInstance berikutnya harus membuat instance baru
      const newClient = PrismaService.getInstance();
      expect(newClient).toBeDefined();

      vi.restoreAllMocks();
    });
  });
});
