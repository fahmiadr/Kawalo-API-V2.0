import { PttRedisRepository } from '@features/ptt/infrastructure/ptt-redis.repository';
import type Redis from 'ioredis';
import { beforeEach, describe, expect, it, vi } from 'vitest';

describe('PttRedisRepository Unit Tests', () => {
  let mockRedis: {
    eval: ReturnType<typeof vi.fn>;
    get: ReturnType<typeof vi.fn>;
    del: ReturnType<typeof vi.fn>;
    sadd: ReturnType<typeof vi.fn>;
    srem: ReturnType<typeof vi.fn>;
    scard: ReturnType<typeof vi.fn>;
    smembers: ReturnType<typeof vi.fn>;
    expire: ReturnType<typeof vi.fn>;
  };
  let repository: PttRedisRepository;
  const groupUid = 'group_102';

  beforeEach(() => {
    mockRedis = {
      eval: vi.fn(),
      get: vi.fn(),
      del: vi.fn().mockResolvedValue(1),
      sadd: vi.fn().mockResolvedValue(1),
      srem: vi.fn().mockResolvedValue(1),
      scard: vi.fn().mockResolvedValue(1),
      smembers: vi.fn().mockResolvedValue([]),
      expire: vi.fn().mockResolvedValue(1),
    };

    repository = new PttRedisRepository(mockRedis as unknown as Redis);
  });

  describe('Key Formatting Convention (/redis-core)', () => {
    it('harus menghasilkan key floor dan members yang konsisten', () => {
      expect(repository.getFloorKey(groupUid)).toBe('ptt:floor:group_102');
      expect(repository.getMembersKey(groupUid)).toBe('ptt:room:group_102:members');
    });
  });

  describe('acquireFloor (Atomic Lua Script)', () => {
    it('harus mengabulkan floor grant saat channel sedang kosong (IDLE)', async () => {
      mockRedis.eval.mockResolvedValueOnce(JSON.stringify({ granted: true, preempted: false }));

      const speaker = {
        userId: 'usr_guard_1',
        name: 'Guard A',
        role: 'GUARD',
      };

      const result = await repository.acquireFloor(groupUid, speaker, 2);

      expect(result.granted).toBe(true);
      expect(result.preempted).toBe(false);
      expect(mockRedis.eval).toHaveBeenCalledWith(
        expect.stringContaining("redis.call('GET', KEYS[1])"),
        1,
        'ptt:floor:group_102',
        expect.stringContaining('usr_guard_1'),
        'GUARD',
        2,
      );
    });

    it('harus mengabulkan Admin Preemption saat Admin mengambil alih transmisi Guard', async () => {
      mockRedis.eval.mockResolvedValueOnce(
        JSON.stringify({
          granted: true,
          preempted: true,
          previousSpeaker: { userId: 'usr_guard_1', name: 'Guard A', role: 'GUARD' },
        }),
      );

      const adminSpeaker = {
        userId: 'usr_admin_1',
        name: 'Captain Hendra',
        role: 'ADMIN',
      };

      const result = await repository.acquireFloor(groupUid, adminSpeaker, 2);

      expect(result.granted).toBe(true);
      expect(result.preempted).toBe(true);
      expect(result.previousSpeaker?.userId).toBe('usr_guard_1');
    });

    it('harus menolak floor saat channel sedang sibuk (FIFO)', async () => {
      mockRedis.eval.mockResolvedValueOnce(
        JSON.stringify({
          granted: false,
          preempted: false,
          currentSpeaker: { userId: 'usr_guard_1', name: 'Guard A', role: 'GUARD' },
        }),
      );

      const secondGuard = {
        userId: 'usr_guard_2',
        name: 'Guard B',
        role: 'GUARD',
      };

      const result = await repository.acquireFloor(groupUid, secondGuard, 2);

      expect(result.granted).toBe(false);
      expect(result.preempted).toBe(false);
      expect(result.currentSpeaker?.userId).toBe('usr_guard_1');
    });
  });

  describe('extendFloorLease (1Hz Debounced Renewal)', () => {
    it('harus mengembalikan true jika lease berhasil diperpanjang oleh pemilik floor yang sah', async () => {
      mockRedis.eval.mockResolvedValueOnce(1);

      const success = await repository.extendFloorLease(groupUid, 'usr_guard_1', 2);
      expect(success).toBe(true);
      expect(mockRedis.eval).toHaveBeenCalledWith(
        expect.stringContaining("redis.call('EXPIRE', KEYS[1], ARGV[2])"),
        1,
        'ptt:floor:group_102',
        'usr_guard_1',
        2,
      );
    });

    it('harus mengembalikan false jika perpanjangan dilakukan bukan oleh pemilik floor', async () => {
      mockRedis.eval.mockResolvedValueOnce(0);

      const success = await repository.extendFloorLease(groupUid, 'usr_other', 2);
      expect(success).toBe(false);
    });
  });

  describe('releaseFloor', () => {
    it('harus menghapus floor key jika dilepaskan oleh pemilik yang sah', async () => {
      mockRedis.eval.mockResolvedValueOnce(1);

      const success = await repository.releaseFloor(groupUid, 'usr_guard_1');
      expect(success).toBe(true);
    });
  });

  describe('getCurrentFloor', () => {
    it('harus mengembalikan data speaker aktif jika key ada', async () => {
      mockRedis.get.mockResolvedValueOnce(
        JSON.stringify({
          userId: 'usr_guard_1',
          name: 'Guard A',
          role: 'GUARD',
        }),
      );

      const speaker = await repository.getCurrentFloor(groupUid);
      expect(speaker).toEqual({
        userId: 'usr_guard_1',
        name: 'Guard A',
        role: 'GUARD',
      });
    });

    it('harus mengembalikan null jika channel IDLE', async () => {
      mockRedis.get.mockResolvedValueOnce(null);

      const speaker = await repository.getCurrentFloor(groupUid);
      expect(speaker).toBeNull();
    });
  });

  describe('Presence Tracking via Redis Sets', () => {
    it('harus menambah member ke set dan menyetel TTL 24 jam', async () => {
      mockRedis.scard.mockResolvedValueOnce(3);

      const count = await repository.addMember(groupUid, 'usr_1');
      expect(mockRedis.sadd).toHaveBeenCalledWith('ptt:room:group_102:members', 'usr_1');
      expect(mockRedis.expire).toHaveBeenCalledWith('ptt:room:group_102:members', 86400);
      expect(count).toBe(3);
    });

    it('harus menghapus member dari set saat disconnect', async () => {
      mockRedis.scard.mockResolvedValueOnce(2);

      const count = await repository.removeMember(groupUid, 'usr_1');
      expect(mockRedis.srem).toHaveBeenCalledWith('ptt:room:group_102:members', 'usr_1');
      expect(count).toBe(2);
    });

    it('harus mengembalikan daftar dan jumlah member', async () => {
      mockRedis.smembers.mockResolvedValueOnce(['usr_1', 'usr_2']);
      mockRedis.scard.mockResolvedValueOnce(2);

      const members = await repository.getMembers(groupUid);
      const count = await repository.getMemberCount(groupUid);

      expect(members).toEqual(['usr_1', 'usr_2']);
      expect(count).toBe(2);
    });
  });
});
