import { RedisService } from '@core/database/redis.service';
import { LoginRateLimiterService } from '@features/auth/infrastructure/login-rate-limiter.service';
import { beforeEach, describe, expect, it, vi } from 'vitest';

describe('LoginRateLimiterService Unit Tests', () => {
  const mockRedis = {
    get: vi.fn(),
    set: vi.fn(),
    incr: vi.fn(),
    expire: vi.fn(),
    ttl: vi.fn(),
    del: vi.fn(),
  };

  beforeEach(() => {
    vi.clearAllMocks();
    vi.spyOn(RedisService, 'getInstance').mockReturnValue(
      mockRedis as unknown as ReturnType<typeof RedisService.getInstance>,
    );
  });

  describe('checkIpRateLimit', () => {
    it('harus mengizinkan jika count masih di bawah ambang batas (10)', async () => {
      mockRedis.get.mockResolvedValueOnce('5');

      const result = await LoginRateLimiterService.checkIpRateLimit('192.168.1.10');
      expect(result.isBlocked).toBe(false);
      expect(result.remaining).toBe(5);
    });

    it('harus memblokir jika count mencapai atau melebihi 10', async () => {
      mockRedis.get.mockResolvedValueOnce('10');
      mockRedis.ttl.mockResolvedValueOnce(600);

      const result = await LoginRateLimiterService.checkIpRateLimit('192.168.1.10');
      expect(result.isBlocked).toBe(true);
      expect(result.remaining).toBe(0);
      expect(result.retryAfterSeconds).toBe(600);
    });
  });

  describe('checkEmailLock', () => {
    it('harus mengembalikan isLocked false jika akun belum dikunci', async () => {
      mockRedis.get.mockResolvedValueOnce(null);

      const result = await LoginRateLimiterService.checkEmailLock('personil@kawalo.com');
      expect(result.isLocked).toBe(false);
      expect(result.remainingLockoutSeconds).toBe(0);
    });

    it('harus mengembalikan isLocked true jika akun sedang dikunci', async () => {
      mockRedis.get.mockResolvedValueOnce('1');
      mockRedis.ttl.mockResolvedValueOnce(750);

      const result = await LoginRateLimiterService.checkEmailLock('personil@kawalo.com');
      expect(result.isLocked).toBe(true);
      expect(result.remainingLockoutSeconds).toBe(750);
    });
  });

  describe('recordFailedAttempt', () => {
    it('harus menaikkan counter kegagalan dan belum mengunci jika belum mencapai 5 kali', async () => {
      mockRedis.incr.mockResolvedValueOnce(1); // IP incr (opsional)
      mockRedis.incr.mockResolvedValueOnce(3); // Email fail incr

      const result = await LoginRateLimiterService.recordFailedAttempt(
        'personil@kawalo.com',
        '192.168.1.10',
      );

      expect(result.isNowLocked).toBe(false);
      expect(result.remainingAttempts).toBe(2);
      expect(mockRedis.set).not.toHaveBeenCalled();
    });

    it('harus mengunci akun saat kegagalan mencapai 5 kali', async () => {
      mockRedis.incr.mockResolvedValueOnce(2); // IP incr
      mockRedis.incr.mockResolvedValueOnce(5); // Email fail incr ke 5

      const result = await LoginRateLimiterService.recordFailedAttempt(
        'personil@kawalo.com',
        '192.168.1.10',
      );

      expect(result.isNowLocked).toBe(true);
      expect(result.remainingAttempts).toBe(0);
      // Memastikan lock key diset dengan TTL lockout
      expect(mockRedis.set).toHaveBeenCalledWith(
        'rl:login:email:locked:personil@kawalo.com',
        '1',
        'EX',
        expect.any(Number),
      );
      // Memastikan fail counter dihapus
      expect(mockRedis.del).toHaveBeenCalledWith('rl:login:email:fails:personil@kawalo.com');
    });
  });

  describe('resetLoginAttempts', () => {
    it('harus menghapus fail key dan lock key saat login sukses', async () => {
      await LoginRateLimiterService.resetLoginAttempts('Personil@Kawalo.com');

      expect(mockRedis.del).toHaveBeenCalledWith('rl:login:email:fails:personil@kawalo.com');
      expect(mockRedis.del).toHaveBeenCalledWith('rl:login:email:locked:personil@kawalo.com');
    });
  });
});
