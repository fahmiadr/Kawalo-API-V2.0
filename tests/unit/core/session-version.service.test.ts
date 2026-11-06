import { redis } from '@core/database/redis.service';
import { SessionVersionService } from '@core/security/session-version.service';
import { beforeEach, describe, expect, it } from 'vitest';

describe('SessionVersionService Unit Tests (/redis-core compliant)', () => {
  beforeEach(async () => {
    await redis.del(SessionVersionService.getKey('101'), SessionVersionService.getKey('102'));
  });

  it('getKey menghasilkan key yang sesuai dengan hierarki /redis-core', () => {
    expect(SessionVersionService.getKey('101')).toBe('auth:user:101:session_ver');
  });

  it('getOrInitVersion menginisialisasi versi ke 1 dengan TTL 14 hari jika belum ada di Redis', async () => {
    const ver = await SessionVersionService.getOrInitVersion('101');
    expect(ver).toBe(1);

    const ttl = await redis.ttl(SessionVersionService.getKey('101'));
    expect(ttl).toBeGreaterThan(0);
    expect(ttl).toBeLessThanOrEqual(SessionVersionService.TTL_SECONDS);

    // Pemanggilan berikutnya mengembalikan versi yang sama dan merotasi sliding TTL
    const ver2 = await SessionVersionService.getOrInitVersion('101');
    expect(ver2).toBe(1);
  });

  it('incrementVersion menaikkan versi secara atomik (INCR) dan memperbarui TTL', async () => {
    await SessionVersionService.getOrInitVersion('101'); // ver = 1
    const newVer = await SessionVersionService.incrementVersion('101');
    expect(newVer).toBe(2);

    const check = await redis.get(SessionVersionService.getKey('101'));
    expect(check).toBe('2');

    const ttl = await redis.ttl(SessionVersionService.getKey('101'));
    expect(ttl).toBeGreaterThan(0);
  });

  it('validateOrRehydrate mengembalikan true jika versi token sama atau lebih baru dari Redis', async () => {
    await redis.set(SessionVersionService.getKey('101'), '2');

    // Token dengan versi 2 sah
    const valid = await SessionVersionService.validateOrRehydrate('101', 2);
    expect(valid).toBe(true);

    // Token dengan versi 3 (lebih baru) sah
    const validNewer = await SessionVersionService.validateOrRehydrate('101', 3);
    expect(validNewer).toBe(true);
  });

  it('validateOrRehydrate mengembalikan false jika versi token lebih kecil dari Redis (Stale Session)', async () => {
    // Admin menaikkan versi user di Redis menjadi 3
    await redis.set(SessionVersionService.getKey('101'), '3');

    // Token lama yang masih membawa versi 1 atau 2 harus ditolak
    const validOld = await SessionVersionService.validateOrRehydrate('101', 1);
    expect(validOld).toBe(false);

    const validOld2 = await SessionVersionService.validateOrRehydrate('101', 2);
    expect(validOld2).toBe(false);
  });

  it('validateOrRehydrate tahan terhadap Redis Restart / Cold Start (Self-Healing Rehydration)', async () => {
    // Simulasi Redis Restart / flush: key belum ada sama sekali di Redis
    const key = SessionVersionService.getKey('102');
    expect(await redis.get(key)).toBeNull();

    // User mengirim token yang valid membawa versi 2
    const valid = await SessionVersionService.validateOrRehydrate('102', 2);
    expect(valid).toBe(true);

    // Key di Redis otomatis direhidrasi dengan versi 2 dan TTL 14 hari
    const cachedVer = await redis.get(key);
    expect(cachedVer).toBe('2');
    const ttl = await redis.ttl(key);
    expect(ttl).toBeGreaterThan(0);
  });

  it('melemparkan error jika userId kosong pada operasi mutasi (Fail-Fast)', async () => {
    await expect(SessionVersionService.getOrInitVersion('')).rejects.toThrow('UserId wajib diisi');
    await expect(SessionVersionService.incrementVersion('')).rejects.toThrow('UserId wajib diisi');
    expect(await SessionVersionService.validateOrRehydrate('', 1)).toBe(false);
  });
});
