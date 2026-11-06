import { redis } from '@core/database/redis.service';
import { AuthConfig } from '@features/auth/config/auth.config';

/**
 * Service pengelola Session Versioning berbasis Redis.
 * Mengikuti pedoman /redis-core:
 * - Data Structure: String (Atomic INCR/DECR, SET/GET O(1))
 * - Key Naming: auth:user:{userId}:session_ver (lowercase, colon-separated, consistent hierarchy)
 * - TTL: Sama dengan masa aktif Refresh Token (14 hari). Sliding window saat user aktif,
 *        dan otomatis dihapus oleh Redis expiry saat user inaktif agar terhindar dari memory bloat.
 * - Resilience / Self-Healing: Jika Redis restart dan key berstatus null, sistem melakukan
 *        auto-rehydration dari token yang sah tanpa menyebabkan logout massal (Zero Disruption).
 */
export class SessionVersionService {
  /**
   * Menghasilkan key Redis sesuai konvensi penamaan standar /redis-core.
   */
  public static getKey(userId: string): string {
    return `auth:user:${userId}:session_ver`;
  }

  /**
   * Durasi retensi sesi di Redis (14 hari = 1.209.600 detik).
   */
  public static get TTL_SECONDS(): number {
    return AuthConfig.REFRESH_TOKEN_EXPIRES_IN;
  }

  /**
   * Mengambil versi sesi aktif atau menginisialisasinya ke 1 jika belum ada.
   * Digunakan saat menerbitkan Access Token baru (Login, Refresh, Switch Group).
   */
  public static async getOrInitVersion(userId: string): Promise<number> {
    if (!userId) {
      throw new Error('UserId wajib diisi untuk menginisialisasi versi sesi');
    }

    const key = this.getKey(userId);
    const existing = await redis.get(key);

    if (existing !== null) {
      await redis.expire(key, this.TTL_SECONDS);
      return Number(existing);
    }

    await redis.set(key, '1', 'EX', this.TTL_SECONDS);
    return 1;
  }

  /**
   * Menaikkan versi sesi secara atomik di Redis (INCR) dan memperbarui TTL.
   * Digunakan untuk:
   * 1. Mengubah wewenang / hak akses anggota grup oleh Admin (Real-time Sync).
   * 2. Mencabut akses saat user dinonaktifkan atau dikeluarkan dari grup (Immediate Revocation).
   * 3. Mencabut access token yang sedang aktif saat pengguna melakukan Logout.
   */
  public static async incrementVersion(userId: string): Promise<number> {
    if (!userId) {
      throw new Error('UserId wajib diisi untuk menaikkan versi sesi');
    }

    const key = this.getKey(userId);
    const newVersion = await redis.incr(key);
    await redis.expire(key, this.TTL_SECONDS);
    return newVersion;
  }

  /**
   * Memvalidasi apakah versi sesi yang dibawa token masih sah dibandingkan Redis.
   *
   * Ketahanan Redis Reload / Restart:
   * Jika key di Redis null (akibat Redis restart / flush), sistem tidak langsung
   * menolak token RS256 yang sah. Sistem melakukan rehidrasi mandiri (Self-Healing)
   * dengan menyetel kembali key di Redis sesuai versi token saat ini (NX) dengan TTL 14 hari.
   *
   * Mengembalikan false HANYA JIKA versi token lebih kecil daripada versi di Redis (Stale/Revoked).
   */
  public static async validateOrRehydrate(
    userId: string,
    tokenVersion?: number | null,
  ): Promise<boolean> {
    if (!userId || typeof tokenVersion !== 'number' || tokenVersion <= 0) {
      return false;
    }

    const key = this.getKey(userId);
    const cached = await redis.get(key);

    if (cached === null) {
      // Rehidrasi mandiri saat Redis baru restart/cold start
      await redis.set(key, String(tokenVersion), 'EX', this.TTL_SECONDS, 'NX');
      return true;
    }

    return tokenVersion >= Number(cached);
  }
}
