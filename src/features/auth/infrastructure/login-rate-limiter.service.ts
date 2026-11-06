import { RedisService } from '@core/database/redis.service';
import { AuthConfig } from '@features/auth/config/auth.config';

export interface IpRateLimitStatus {
  isBlocked: boolean;
  remaining: number;
  retryAfterSeconds: number;
}

export interface EmailLockStatus {
  isLocked: boolean;
  remainingLockoutSeconds: number;
}

export interface RecordFailResult {
  isNowLocked: boolean;
  remainingAttempts: number;
  lockoutDurationSeconds: number;
}

export class LoginRateLimiterService {
  private static get redis() {
    return RedisService.getInstance();
  }

  private static normalizeEmail(email: string): string {
    return email.trim().toLowerCase();
  }

  private static getIpKey(clientIp: string): string {
    return `rl:login:ip:${clientIp.trim()}`;
  }

  private static getEmailFailKey(email: string): string {
    return `rl:login:email:fails:${LoginRateLimiterService.normalizeEmail(email)}`;
  }

  private static getEmailLockKey(email: string): string {
    return `rl:login:email:locked:${LoginRateLimiterService.normalizeEmail(email)}`;
  }

  /**
   * Layer 1: Memeriksa apakah IP klien telah melampaui kuota permintaan login.
   */
  public static async checkIpRateLimit(clientIp: string): Promise<IpRateLimitStatus> {
    const key = LoginRateLimiterService.getIpKey(clientIp);
    const countStr = await LoginRateLimiterService.redis.get(key);
    const count = countStr ? Number.parseInt(countStr, 10) : 0;

    if (count >= AuthConfig.LOGIN_RATE_LIMIT_IP_MAX) {
      const ttl = await LoginRateLimiterService.redis.ttl(key);
      return {
        isBlocked: true,
        remaining: 0,
        retryAfterSeconds: ttl > 0 ? ttl : AuthConfig.LOGIN_RATE_LIMIT_DURATION,
      };
    }

    return {
      isBlocked: false,
      remaining: Math.max(0, AuthConfig.LOGIN_RATE_LIMIT_IP_MAX - count),
      retryAfterSeconds: 0,
    };
  }

  /**
   * Menambahkan hitungan upaya login pada IP tertentu.
   */
  public static async incrementIpCount(clientIp: string): Promise<void> {
    const key = LoginRateLimiterService.getIpKey(clientIp);
    const current = await LoginRateLimiterService.redis.incr(key);
    if (current === 1) {
      await LoginRateLimiterService.redis.expire(key, AuthConfig.LOGIN_RATE_LIMIT_DURATION);
    }
  }

  /**
   * Layer 2: Memeriksa apakah akun email personil sedang dalam status terkunci.
   */
  public static async checkEmailLock(email: string): Promise<EmailLockStatus> {
    const lockKey = LoginRateLimiterService.getEmailLockKey(email);
    const isLocked = await LoginRateLimiterService.redis.get(lockKey);

    if (isLocked) {
      const ttl = await LoginRateLimiterService.redis.ttl(lockKey);
      return {
        isLocked: true,
        remainingLockoutSeconds: ttl > 0 ? ttl : AuthConfig.LOGIN_LOCKOUT_DURATION,
      };
    }

    return {
      isLocked: false,
      remainingLockoutSeconds: 0,
    };
  }

  /**
   * Mencatat kegagalan kata sandi dan mengunci akun jika mencapai ambang batas (5 kali gagal).
   */
  public static async recordFailedAttempt(
    email: string,
    clientIp?: string,
  ): Promise<RecordFailResult> {
    if (clientIp) {
      await LoginRateLimiterService.incrementIpCount(clientIp);
    }

    const failKey = LoginRateLimiterService.getEmailFailKey(email);
    const lockKey = LoginRateLimiterService.getEmailLockKey(email);

    const fails = await LoginRateLimiterService.redis.incr(failKey);
    if (fails === 1) {
      await LoginRateLimiterService.redis.expire(failKey, AuthConfig.LOGIN_LOCKOUT_DURATION);
    }

    if (fails >= AuthConfig.LOGIN_MAX_FAILED_ATTEMPTS) {
      // Pasang kunci akun selama durasi lockout (default: 15 menit)
      await LoginRateLimiterService.redis.set(
        lockKey,
        '1',
        'EX',
        AuthConfig.LOGIN_LOCKOUT_DURATION,
      );
      // Reset fail counter karena sudah resmi dikunci
      await LoginRateLimiterService.redis.del(failKey);

      return {
        isNowLocked: true,
        remainingAttempts: 0,
        lockoutDurationSeconds: AuthConfig.LOGIN_LOCKOUT_DURATION,
      };
    }

    return {
      isNowLocked: false,
      remainingAttempts: Math.max(0, AuthConfig.LOGIN_MAX_FAILED_ATTEMPTS - fails),
      lockoutDurationSeconds: 0,
    };
  }

  /**
   * Menghapus seluruh riwayat kegagalan dan kunci akun saat login berhasil.
   */
  public static async resetLoginAttempts(email: string): Promise<void> {
    const failKey = LoginRateLimiterService.getEmailFailKey(email);
    const lockKey = LoginRateLimiterService.getEmailLockKey(email);

    await Promise.all([
      LoginRateLimiterService.redis.del(failKey),
      LoginRateLimiterService.redis.del(lockKey),
    ]);
  }
}
