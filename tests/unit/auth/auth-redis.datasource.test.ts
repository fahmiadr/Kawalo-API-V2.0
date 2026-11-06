import { AuthRedisDataSource } from '@features/auth/data/datasources/auth-redis.datasource';
import { RefreshTokenEntity } from '@features/auth/domain/entities/auth-tokens.entity';
import type Redis from 'ioredis';
import { beforeEach, describe, expect, it, vi } from 'vitest';

describe('AuthRedisDataSource Unit Tests', () => {
  let mockPipeline: {
    set: ReturnType<typeof vi.fn>;
    setex: ReturnType<typeof vi.fn>;
    zremrangebyscore: ReturnType<typeof vi.fn>;
    zadd: ReturnType<typeof vi.fn>;
    expire: ReturnType<typeof vi.fn>;
    del: ReturnType<typeof vi.fn>;
    zrem: ReturnType<typeof vi.fn>;
    exec: ReturnType<typeof vi.fn>;
  };
  let mockRedis: {
    get: ReturnType<typeof vi.fn>;
    del: ReturnType<typeof vi.fn>;
    eval: ReturnType<typeof vi.fn>;
    zrange: ReturnType<typeof vi.fn>;
    zrem: ReturnType<typeof vi.fn>;
    pipeline: ReturnType<typeof vi.fn>;
    set: ReturnType<typeof vi.fn>;
  };
  let dataSource: AuthRedisDataSource;

  beforeEach(() => {
    mockPipeline = {
      set: vi.fn().mockReturnThis(),
      setex: vi.fn().mockReturnThis(),
      zremrangebyscore: vi.fn().mockReturnThis(),
      zadd: vi.fn().mockReturnThis(),
      expire: vi.fn().mockReturnThis(),
      del: vi.fn().mockReturnThis(),
      zrem: vi.fn().mockReturnThis(),
      exec: vi.fn().mockResolvedValue([]),
    };

    mockRedis = {
      get: vi.fn(),
      del: vi.fn().mockResolvedValue(1),
      eval: vi.fn(),
      zrange: vi.fn().mockResolvedValue([]),
      zrem: vi.fn().mockResolvedValue(1),
      pipeline: vi.fn().mockReturnValue(mockPipeline),
      set: vi.fn(),
    };

    dataSource = new AuthRedisDataSource(mockRedis as unknown as Redis);
  });

  describe('saveSession', () => {
    it('harus memetakan RefreshTokenEntity ke RedisSessionData, membersihkan token mati, dan menyimpan ke ZSET via pipelining', async () => {
      const now = Date.now();
      const expiresAt = new Date(now + 3600 * 1000); // 1 jam lagi
      const token = new RefreshTokenEntity({
        userId: '101',
        tokenHash: 'hash_abc123',
        familyId: 'family_xyz',
        expiresAt,
        activeGroupUid: 'GRP-01',
        createdAt: new Date(now),
      });

      await dataSource.saveSession(token);

      expect(mockPipeline.set).toHaveBeenCalledWith(
        'auth:refresh:hash_abc123',
        expect.stringContaining('"user_id":"101"'),
        'EX',
        expect.any(Number),
      );
      expect(mockPipeline.zremrangebyscore).toHaveBeenCalledWith(
        'auth:user:101:sessions',
        '-inf',
        expect.any(Number),
      );
      expect(mockPipeline.zadd).toHaveBeenCalledWith(
        'auth:user:101:sessions',
        Math.floor(expiresAt.getTime() / 1000),
        'hash_abc123',
      );
      expect(mockPipeline.zremrangebyscore).toHaveBeenCalledWith(
        'auth:family:family_xyz:sessions',
        '-inf',
        expect.any(Number),
      );
      expect(mockPipeline.zadd).toHaveBeenCalledWith(
        'auth:family:family_xyz:sessions',
        Math.floor(expiresAt.getTime() / 1000),
        'hash_abc123',
      );
      expect(mockPipeline.expire).toHaveBeenCalledTimes(2);
      expect(mockPipeline.exec).toHaveBeenCalledTimes(1);
    });

    it('harus membersihkan active_group_uid jika bernilai dirty seperti N/A (pure nullable)', async () => {
      const now = Date.now();
      const token = new RefreshTokenEntity({
        userId: '102',
        tokenHash: 'hash_def456',
        familyId: 'family_xyz',
        expiresAt: new Date(now + 3600 * 1000),
        activeGroupUid: 'N/A', // Dirty string
      });

      await dataSource.saveSession(token);

      expect(mockPipeline.set).toHaveBeenCalledWith(
        'auth:refresh:hash_def456',
        expect.stringContaining('"active_group_uid":null'),
        'EX',
        expect.any(Number),
      );
    });

    it('harus mengabaikan simpan sesi jika tokenHash kosong', async () => {
      const token = new RefreshTokenEntity({
        userId: '103',
        tokenHash: '   ',
        familyId: 'family_xyz',
        expiresAt: new Date(),
      });

      await dataSource.saveSession(token);
      expect(mockPipeline.set).not.toHaveBeenCalled();
    });

    it('harus mengabaikan simpan sesi jika userId atau familyId korup (Zero Data Poisoning ke Redis)', async () => {
      const tokenCorruptUser = new RefreshTokenEntity({
        userId: '   ',
        tokenHash: 'hash_corrupt',
        familyId: 'family_xyz',
        expiresAt: new Date(Date.now() + 3600 * 1000),
      });

      await dataSource.saveSession(tokenCorruptUser);
      expect(mockPipeline.set).not.toHaveBeenCalled();

      const tokenCorruptFamily = new RefreshTokenEntity({
        userId: '104',
        tokenHash: 'hash_corrupt2',
        familyId: 'N/A', // Dirty string
        expiresAt: new Date(Date.now() + 3600 * 1000),
      });

      await dataSource.saveSession(tokenCorruptFamily);
      expect(mockPipeline.set).not.toHaveBeenCalled();
    });
  });

  describe('getSession', () => {
    it('harus memetakan data Redis yang valid ke RefreshTokenEntity (100% pra-sanitasi)', async () => {
      const nowUnix = Math.floor(Date.now() / 1000);
      mockRedis.get.mockResolvedValueOnce(
        JSON.stringify({
          user_id: '101',
          family_id: 'family_abc',
          active_group_uid: 'GRP-01',
          created_at: nowUnix,
          expires_at: nowUnix + 3600,
        }),
      );

      const session = await dataSource.getSession('valid_hash');

      expect(session).toBeInstanceOf(RefreshTokenEntity);
      expect(session?.userId).toBe('101');
      expect(session?.familyId).toBe('family_abc');
      expect(session?.activeGroupUid).toBe('GRP-01');
      expect(session?.tokenHash).toBe('valid_hash');
      expect(session?.expiresAt.getTime()).toBe((nowUnix + 3600) * 1000);
    });

    it('harus menormalisasi activeGroupUid kotor ke null murni (Zero Data Poisoning)', async () => {
      const nowUnix = Math.floor(Date.now() / 1000);
      mockRedis.get.mockResolvedValueOnce(
        JSON.stringify({
          user_id: '101',
          family_id: 'family_abc',
          active_group_uid: 'N/A', // Kotor dari DB/Redis
          created_at: nowUnix,
          expires_at: nowUnix + 3600,
        }),
      );

      const session = await dataSource.getSession('valid_hash');
      expect(session?.activeGroupUid).toBeNull();
    });

    it('harus mengembalikan null jika data Redis corrupt demi Zero Data Hallucination', async () => {
      // Kasus 1: user_id kosong
      mockRedis.get.mockResolvedValueOnce(
        JSON.stringify({
          user_id: '   ',
          family_id: 'family_abc',
          active_group_uid: null,
          created_at: 1000,
          expires_at: 2000,
        }),
      );
      expect(await dataSource.getSession('h1')).toBeNull();

      // Kasus 2: family_id 'N/A'
      mockRedis.get.mockResolvedValueOnce(
        JSON.stringify({
          user_id: '101',
          family_id: 'N/A',
          active_group_uid: null,
          created_at: 1000,
          expires_at: 2000,
        }),
      );
      expect(await dataSource.getSession('h2')).toBeNull();

      // Kasus 3: expires_at bukan angka valid / 0
      mockRedis.get.mockResolvedValueOnce(
        JSON.stringify({
          user_id: '101',
          family_id: 'family_abc',
          active_group_uid: null,
          created_at: 1000,
          expires_at: 0,
        }),
      );
      expect(await dataSource.getSession('h3')).toBeNull();
    });

    it('harus mengembalikan null jika tokenHash tidak valid (input guard)', async () => {
      expect(await dataSource.getSession('')).toBeNull();
      expect(await dataSource.getSession('   ')).toBeNull();
      expect(mockRedis.get).not.toHaveBeenCalled();
    });
  });

  describe('getRotatedFamilyId', () => {
    it('harus mengembalikan family ID yang bersih atau null via auth:tombstone lookup', async () => {
      mockRedis.get.mockResolvedValueOnce('family_compromised');
      expect(await dataSource.getRotatedFamilyId('rotated_hash')).toBe('family_compromised');
      expect(mockRedis.get).toHaveBeenCalledWith('auth:tombstone:rotated_hash');

      mockRedis.get.mockResolvedValueOnce('N/A');
      expect(await dataSource.getRotatedFamilyId('dirty_hash')).toBeNull();

      expect(await dataSource.getRotatedFamilyId('   ')).toBeNull();
    });
  });

  describe('getGraceTokens', () => {
    it('harus mengembalikan AuthTokensEntity yang valid atau null via auth:grace lookup', async () => {
      mockRedis.get.mockResolvedValueOnce(
        JSON.stringify({
          accessToken: 'at_123',
          refreshToken: 'rt_123',
          tokenType: 'Bearer',
          expiresIn: 3600,
        }),
      );
      const token = await dataSource.getGraceTokens('grace_hash');
      expect(token).not.toBeNull();
      expect(token?.accessToken).toBe('at_123');
      expect(token?.refreshToken).toBe('rt_123');
      expect(mockRedis.get).toHaveBeenCalledWith('auth:grace:grace_hash');

      mockRedis.get.mockResolvedValueOnce('invalid json');
      expect(await dataSource.getGraceTokens('corrupt_hash')).toBeNull();

      expect(await dataSource.getGraceTokens('   ')).toBeNull();
    });
  });

  describe('rotateSession', () => {
    it('harus memanggil skrip Lua atomik dengan 6 KEYS dan 9 ARGV', async () => {
      mockRedis.eval.mockResolvedValueOnce(
        JSON.stringify({
          user_id: '101',
          family_id: 'family_1',
          active_group_uid: null,
          created_at: 1000,
          expires_at: 2000,
        }),
      );

      const now = Date.now();
      const newToken = new RefreshTokenEntity({
        userId: '101',
        tokenHash: 'new_hash',
        familyId: 'family_1',
        expiresAt: new Date(now + 3600 * 1000),
      });

      const result = await dataSource.rotateSession('old_hash', newToken);

      expect(result).toBe(true);
      expect(mockRedis.eval).toHaveBeenCalledWith(
        expect.any(String),
        6,
        'auth:refresh:old_hash',
        'auth:refresh:new_hash',
        'auth:tombstone:old_hash',
        'auth:user:101:sessions',
        'auth:family:family_1:sessions',
        'auth:grace:old_hash',
        expect.any(String), // newVal
        expect.any(String), // ttl
        'family_1',
        'old_hash',
        'new_hash',
        expect.any(String), // expires_at
        expect.any(String), // now_unix
        '', // grace_val
        '15', // grace_ttl
      );
    });

    it('harus menolak rotasi jika oldHash atau newHash kosong', async () => {
      const newToken = new RefreshTokenEntity({
        userId: '101',
        tokenHash: '   ',
        familyId: 'family_1',
        expiresAt: new Date(),
      });

      expect(await dataSource.rotateSession('old_hash', newToken)).toBe(false);
      expect(await dataSource.rotateSession('   ', newToken)).toBe(false);
      expect(mockRedis.eval).not.toHaveBeenCalled();
    });

    it('harus menolak rotasi jika userId atau familyId pada newToken korup (Zero Poisoning)', async () => {
      const corruptToken = new RefreshTokenEntity({
        userId: '101',
        tokenHash: 'new_valid_hash',
        familyId: 'N/A', // Dirty string
        expiresAt: new Date(Date.now() + 3600 * 1000),
      });

      expect(await dataSource.rotateSession('old_hash', corruptToken)).toBe(false);
      expect(mockRedis.eval).not.toHaveBeenCalled();
    });
  });

  describe('distributed locks & session revocations', () => {
    it('harus mengabaikan acquireLock atau releaseLock jika key atau lockValue kosong', async () => {
      expect(await dataSource.acquireLock('', 'val')).toBe(false);
      expect(await dataSource.acquireLock('key', '   ')).toBe(false);
      expect(mockRedis.set).not.toHaveBeenCalled();

      await dataSource.releaseLock('', 'val');
      await dataSource.releaseLock('key', '   ');
      expect(mockRedis.eval).not.toHaveBeenCalled();
    });

    it('harus mengeksekusi acquireLock dan releaseLock yang valid', async () => {
      mockRedis.set.mockResolvedValueOnce('OK');
      expect(await dataSource.acquireLock('refresh:101', 'uuid-val', 5)).toBe(true);
      expect(mockRedis.set).toHaveBeenCalledWith('lock:refresh:101', 'uuid-val', 'EX', 5, 'NX');

      await dataSource.releaseLock('refresh:101', 'uuid-val');
      expect(mockRedis.eval).toHaveBeenCalledWith(
        expect.any(String),
        1,
        'lock:refresh:101',
        'uuid-val',
      );
    });

    it('harus mengeksekusi revokeFamily, deleteSession, dan revokeAllUserSessions menggunakan ZSET dan pipelining', async () => {
      mockRedis.zrange.mockResolvedValueOnce(['h1', 'h2']);
      await dataSource.revokeFamily('fam_1');
      expect(mockRedis.zrange).toHaveBeenCalledWith('auth:family:fam_1:sessions', 0, '-1');
      expect(mockPipeline.del).toHaveBeenCalledWith('auth:refresh:h1');
      expect(mockPipeline.del).toHaveBeenCalledWith('auth:refresh:h2');
      expect(mockPipeline.del).toHaveBeenCalledWith('auth:family:fam_1:sessions');
      expect(mockPipeline.exec).toHaveBeenCalledTimes(1);

      await dataSource.revokeFamily('   ');
      expect(mockPipeline.exec).toHaveBeenCalledTimes(1);

      // 1. deleteSession dengan userId -> harus menggunakan pipeline (1 RTT)
      await dataSource.deleteSession('token_1', 'user_1');
      expect(mockPipeline.del).toHaveBeenCalledWith('auth:refresh:token_1');
      expect(mockPipeline.zrem).toHaveBeenCalledWith('auth:user:user_1:sessions', 'token_1');

      // 2. deleteSession tanpa userId -> langsung DEL tanpa pipeline
      await dataSource.deleteSession('token_2');
      expect(mockRedis.del).toHaveBeenCalledWith('auth:refresh:token_2');

      // 3. deleteSession dengan token kosong -> diabaikan
      await dataSource.deleteSession('   ');

      mockRedis.zrange.mockResolvedValueOnce(['h3']);
      await dataSource.revokeAllUserSessions('user_1');
      expect(mockRedis.zrange).toHaveBeenCalledWith('auth:user:user_1:sessions', 0, '-1');
      expect(mockPipeline.del).toHaveBeenCalledWith('auth:refresh:h3');
      expect(mockPipeline.del).toHaveBeenCalledWith('auth:user:user_1:sessions');

      await dataSource.revokeAllUserSessions('   ');
    });
  });
});
