import { redis } from '@core/database/redis.service';
import { cleanString } from '@core/utils/clean-data.util';
import {
  AuthTokensEntity,
  RefreshTokenEntity,
} from '@features/auth/domain/entities/auth-tokens.entity';
import type Redis from 'ioredis';

export interface RedisSessionData {
  user_id: string;
  family_id: string;
  active_group_uid: string | null;
  created_at: number;
  expires_at: number;
}

export class AuthRedisDataSource {
  private readonly client: Redis;

  constructor(client?: Redis) {
    this.client = client ?? redis;
  }

  /**
   * Menyimpan sesi token aktif ke Redis dengan TTL terhitung.
   * Menggunakan Sorted Set (ZSET) dengan skor timestamp kedaluwarsa untuk pengindeksan sesi,
   * serta pipelining untuk meminimalkan network round-trip (1 round-trip vs 7).
   */
  public async saveSession(token: RefreshTokenEntity): Promise<void> {
    const cleanTokenHash = cleanString(token.tokenHash);
    if (!cleanTokenHash) {
      return;
    }

    const sessionData = this.mapToSessionData(token);
    if (!sessionData) {
      return;
    }

    const ttlSeconds = this.calculateTtlSeconds(sessionData.expires_at);
    const nowUnix = Math.floor(Date.now() / 1000);

    const key = `auth:refresh:${cleanTokenHash}`;
    const userKey = `auth:user:${sessionData.user_id}:sessions`;
    const familyKey = `auth:family:${sessionData.family_id}:sessions`;

    // Pipelining: Seluruh operasi simpan, pembersihan token mati, dan perpanjangan TTL dieksekusi dalam 1 RTT
    const pipe = this.client.pipeline();
    pipe.set(key, JSON.stringify(sessionData), 'EX', ttlSeconds);

    // Bersihkan sesi kedaluwarsa pada user & masukkan sesi baru ke ZSET
    pipe.zremrangebyscore(userKey, '-inf', nowUnix);
    pipe.zadd(userKey, sessionData.expires_at, cleanTokenHash);
    pipe.expire(userKey, ttlSeconds);

    // Bersihkan sesi kedaluwarsa pada family & masukkan sesi baru ke ZSET
    pipe.zremrangebyscore(familyKey, '-inf', nowUnix);
    pipe.zadd(familyKey, sessionData.expires_at, cleanTokenHash);
    pipe.expire(familyKey, ttlSeconds);

    await pipe.exec();
  }

  /**
   * Mengambil sesi token aktif dari Redis dan memetakannya ke RefreshTokenEntity.
   * Dilengkapi guard integritas ketat (Zero Data Poisoning & Zero Data Hallucination).
   */
  public async getSession(tokenHash: string): Promise<RefreshTokenEntity | null> {
    // 1. Sanitasi input token hash
    const cleanTokenHash = cleanString(tokenHash);
    if (!cleanTokenHash) {
      return null;
    }

    // 2. Ambil data mentah dari Redis
    const raw = await this.client.get(`auth:refresh:${cleanTokenHash}`);
    if (!raw) {
      return null;
    }

    let rawData: RedisSessionData;
    try {
      rawData = JSON.parse(raw) as RedisSessionData;
    } catch {
      return null;
    }

    // 3. 100% Sanitasi data mentah ke variabel lokal sebelum digunakan untuk logika apapun
    const userId = cleanString(rawData.user_id);
    const familyId = cleanString(rawData.family_id);
    const activeGroupUid = cleanString(rawData.active_group_uid);
    const createdAtUnix = Number(rawData.created_at);
    const expiresAtUnix = Number(rawData.expires_at);

    // 4. Guard integritas anti-hallucination & anti-corruption
    if (
      !userId ||
      !familyId ||
      Number.isNaN(createdAtUnix) ||
      createdAtUnix <= 0 ||
      Number.isNaN(expiresAtUnix) ||
      expiresAtUnix <= 0
    ) {
      return null;
    }

    const createdAt = new Date(createdAtUnix * 1000);
    const expiresAt = new Date(expiresAtUnix * 1000);

    if (Number.isNaN(createdAt.getTime()) || Number.isNaN(expiresAt.getTime())) {
      return null;
    }

    // 5. Konstruksi RefreshTokenEntity menggunakan data yang 100% bersih & pure nullable
    return new RefreshTokenEntity({
      userId,
      tokenHash: cleanTokenHash,
      familyId,
      expiresAt,
      activeGroupUid,
      createdAt,
    });
  }

  /**
   * Mendeteksi reuse token yang sudah dirotasi melalui lookup tombstone.
   */
  public async getRotatedFamilyId(tokenHash: string): Promise<string | null> {
    const cleanTokenHash = cleanString(tokenHash);
    if (!cleanTokenHash) {
      return null;
    }

    const rawFamilyId = await this.client.get(`auth:tombstone:${cleanTokenHash}`);
    return cleanString(rawFamilyId);
  }

  /**
   * Mengambil token yang diterbitkan sebelumnya jika masih dalam jendela toleransi
   * Grace Period 15 detik (penanganan idempotent untuk race condition / network retransmission).
   */
  public async getGraceTokens(tokenHash: string): Promise<AuthTokensEntity | null> {
    const cleanTokenHash = cleanString(tokenHash);
    if (!cleanTokenHash) {
      return null;
    }

    const raw = await this.client.get(`auth:grace:${cleanTokenHash}`);
    if (!raw) {
      return null;
    }

    try {
      const data = JSON.parse(raw);
      if (
        data &&
        typeof data.accessToken === 'string' &&
        typeof data.refreshToken === 'string' &&
        typeof data.tokenType === 'string' &&
        typeof data.expiresIn === 'number'
      ) {
        return new AuthTokensEntity({
          accessToken: data.accessToken,
          refreshToken: data.refreshToken,
          tokenType: data.tokenType,
          expiresIn: data.expiresIn,
        });
      }
      return null;
    } catch {
      return null;
    }
  }

  /**
   * Melakukan rotasi sesi token secara 100% atomik menggunakan skrip Lua di Redis.
   * Menjamin old token dihapus, tombstone tercatat, new token tersimpan,
   * serta indeks ZSET user & family diperbarui dalam 1 transaksi all-or-nothing (Zero Desync).
   * Mendukung pencatatan Idempotent Grace Period (15 detik) untuk toleransi race condition seluler.
   */
  public async rotateSession(
    oldHash: string,
    newToken: RefreshTokenEntity,
    graceTokens?: AuthTokensEntity,
  ): Promise<boolean> {
    const cleanOldHash = cleanString(oldHash);
    const cleanNewHash = cleanString(newToken.tokenHash);

    if (!cleanOldHash || !cleanNewHash) {
      return false;
    }

    const sessionData = this.mapToSessionData(newToken);
    if (!sessionData) {
      return false;
    }

    const ttlSeconds = this.calculateTtlSeconds(sessionData.expires_at);
    const nowUnix = Math.floor(Date.now() / 1000);

    const oldKey = `auth:refresh:${cleanOldHash}`;
    const newKey = `auth:refresh:${cleanNewHash}`;
    const tombstoneKey = `auth:tombstone:${cleanOldHash}`;
    const userKey = `auth:user:${sessionData.user_id}:sessions`;
    const familyKey = `auth:family:${sessionData.family_id}:sessions`;
    const graceKey = `auth:grace:${cleanOldHash}`;
    const newVal = JSON.stringify(sessionData);

    const graceVal = graceTokens
      ? JSON.stringify({
          accessToken: graceTokens.accessToken,
          refreshToken: graceTokens.refreshToken,
          tokenType: graceTokens.tokenType,
          expiresIn: graceTokens.expiresIn,
        })
      : '';
    const graceTtlSeconds = 15;

    const luaScript = `
      local old_key = KEYS[1]
      local new_key = KEYS[2]
      local tombstone_key = KEYS[3]
      local user_key = KEYS[4]
      local family_key = KEYS[5]
      local grace_key = KEYS[6]

      local new_val = ARGV[1]
      local ttl = tonumber(ARGV[2])
      local family_id = ARGV[3]
      local old_hash = ARGV[4]
      local new_hash = ARGV[5]
      local expires_at = tonumber(ARGV[6])
      local now_unix = tonumber(ARGV[7])
      local grace_val = ARGV[8]
      local grace_ttl = tonumber(ARGV[9])

      local data = redis.call('GET', old_key)
      if not data then
          return nil
      end

      -- 1. Rotasi token & catat tombstone
      redis.call('DEL', old_key)
      redis.call('SET', tombstone_key, family_id, 'EX', ttl)
      redis.call('SET', new_key, new_val, 'EX', ttl)

      -- 1.1 Catat idempotent grace period (15 detik) untuk toleransi race condition jaringan
      if grace_val and grace_val ~= '' and grace_ttl and grace_ttl > 0 then
          redis.call('SET', grace_key, grace_val, 'EX', grace_ttl)
      end

      -- 2. Bersihkan token kedaluwarsa & perbarui indeks ZSET user
      redis.call('ZREMRANGEBYSCORE', user_key, '-inf', now_unix)
      redis.call('ZREM', user_key, old_hash)
      redis.call('ZADD', user_key, expires_at, new_hash)
      redis.call('EXPIRE', user_key, ttl)

      -- 3. Bersihkan token kedaluwarsa & perbarui indeks ZSET family
      redis.call('ZREMRANGEBYSCORE', family_key, '-inf', now_unix)
      redis.call('ZREM', family_key, old_hash)
      redis.call('ZADD', family_key, expires_at, new_hash)
      redis.call('EXPIRE', family_key, ttl)

      return data
    `;

    const result = await this.client.eval(
      luaScript,
      6,
      oldKey,
      newKey,
      tombstoneKey,
      userKey,
      familyKey,
      graceKey,
      newVal,
      ttlSeconds.toString(),
      sessionData.family_id,
      cleanOldHash,
      cleanNewHash,
      sessionData.expires_at.toString(),
      nowUnix.toString(),
      graceVal,
      graceTtlSeconds.toString(),
    );

    return typeof result === 'string';
  }

  /**
   * Mencabut seluruh token dalam satu family (misal: saat terdeteksi reuse).
   * Menggunakan ZRANGE untuk mengambil daftar token aktif dan pipelining untuk bulk deletion.
   */
  public async revokeFamily(familyId: string): Promise<void> {
    const cleanFamilyId = cleanString(familyId);
    if (!cleanFamilyId) {
      return;
    }

    const familyKey = `auth:family:${cleanFamilyId}:sessions`;
    const tokenHashes = await this.client.zrange(familyKey, 0, '-1');

    if (tokenHashes.length > 0) {
      const pipeline = this.client.pipeline();
      for (const hash of tokenHashes) {
        pipeline.del(`auth:refresh:${hash}`);
      }
      pipeline.del(familyKey);
      await pipeline.exec();
    }
  }

  /**
   * Menghapus sesi spesifik berdasarkan token hash.
   * Menggunakan pipelining bila userId disertakan untuk menggabungkan DEL token dan ZREM sesi user dalam 1 RTT.
   */
  public async deleteSession(tokenHash: string, userId?: string): Promise<void> {
    const cleanTokenHash = cleanString(tokenHash);
    if (!cleanTokenHash) {
      return;
    }

    const cleanUserId = cleanString(userId);
    if (cleanUserId) {
      const userKey = `auth:user:${cleanUserId}:sessions`;
      const pipe = this.client.pipeline();
      pipe.del(`auth:refresh:${cleanTokenHash}`);
      pipe.zrem(userKey, cleanTokenHash);
      await pipe.exec();
    } else {
      await this.client.del(`auth:refresh:${cleanTokenHash}`);
    }
  }

  /**
   * Mencabut seluruh sesi aktif milik seorang pengguna (Global Kill Switch).
   * Menggunakan ZRANGE untuk mengambil daftar token aktif dan pipelining untuk bulk deletion.
   */
  public async revokeAllUserSessions(userId: string): Promise<void> {
    const cleanUserId = cleanString(userId);
    if (!cleanUserId) {
      return;
    }

    const userKey = `auth:user:${cleanUserId}:sessions`;
    const tokenHashes = await this.client.zrange(userKey, 0, '-1');

    if (tokenHashes.length > 0) {
      const pipeline = this.client.pipeline();
      for (const hash of tokenHashes) {
        pipeline.del(`auth:refresh:${hash}`);
      }
      pipeline.del(userKey);
      await pipeline.exec();
    }
  }

  /**
   * Mengakuisisi distributed lock untuk mencegah race condition (misal saat rotasi paralel).
   */
  public async acquireLock(key: string, lockValue: string, ttlSeconds = 5): Promise<boolean> {
    const cleanKey = cleanString(key);
    const cleanLockValue = cleanString(lockValue);

    if (!cleanKey || !cleanLockValue) {
      return false;
    }

    const result = await this.client.set(
      `lock:${cleanKey}`,
      cleanLockValue,
      'EX',
      ttlSeconds,
      'NX',
    );
    return result === 'OK';
  }

  /**
   * Melepas distributed lock secara aman via Lua script hanya jika lockValue cocok.
   */
  public async releaseLock(key: string, lockValue: string): Promise<void> {
    const cleanKey = cleanString(key);
    const cleanLockValue = cleanString(lockValue);

    if (!cleanKey || !cleanLockValue) {
      return;
    }

    const luaScript = `
      if redis.call("get", KEYS[1]) == ARGV[1] then
        return redis.call("del", KEYS[1])
      else
        return 0
      end
    `;

    await this.client.eval(luaScript, 1, `lock:${cleanKey}`, cleanLockValue);
  }

  /**
   * Helper private: Memetakan entitas domain ke struktur data Redis (Zero Data Poisoning & Zero Double-Cleaning).
   * Mengembalikan null jika atribut esensial (userId, familyId, createdAt, expiresAt) tidak valid.
   */
  private mapToSessionData(token: RefreshTokenEntity): RedisSessionData | null {
    const userId = cleanString(token.userId);
    const familyId = cleanString(token.familyId);
    const activeGroupUid = cleanString(token.activeGroupUid);
    const createdAtUnix = Math.floor(token.createdAt.getTime() / 1000);
    const expiresAtUnix = Math.floor(token.expiresAt.getTime() / 1000);

    if (
      !userId ||
      !familyId ||
      Number.isNaN(createdAtUnix) ||
      createdAtUnix <= 0 ||
      Number.isNaN(expiresAtUnix) ||
      expiresAtUnix <= 0
    ) {
      return null;
    }

    return {
      user_id: userId,
      family_id: familyId,
      active_group_uid: activeGroupUid,
      created_at: createdAtUnix,
      expires_at: expiresAtUnix,
    };
  }

  /**
   * Helper private: Menghitung sisa TTL detik berdasarkan timestamp kedaluwarsa.
   * Menjamin nilai minimal 1 detik untuk mencegah error SET EX pada Redis.
   */
  private calculateTtlSeconds(expiresAtUnix: number): number {
    return Math.max(1, expiresAtUnix - Math.floor(Date.now() / 1000));
  }
}
