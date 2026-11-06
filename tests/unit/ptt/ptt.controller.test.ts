import { PttRedisRepository } from '@features/ptt/infrastructure/ptt-redis.repository';
import { PttRoomRegistry } from '@features/ptt/infrastructure/ptt-room-registry';
import { PttController } from '@features/ptt/presentation/ptt.controller';
import type Redis from 'ioredis';
import { afterEach, beforeEach, describe, expect, it, vi } from 'vitest';

describe('PttController Unit Tests', () => {
  let controller: PttController;
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
  let redisRepo: PttRedisRepository;
  let roomRegistry: PttRoomRegistry;
  const groupUid = 'group_cluster_102';

  beforeEach(() => {
    vi.useFakeTimers();

    mockRedis = {
      eval: vi.fn(),
      get: vi.fn().mockResolvedValue(null),
      del: vi.fn().mockResolvedValue(1),
      sadd: vi.fn().mockResolvedValue(1),
      srem: vi.fn().mockResolvedValue(1),
      scard: vi.fn().mockResolvedValue(1),
      smembers: vi.fn().mockResolvedValue([]),
      expire: vi.fn().mockResolvedValue(1),
    };

    redisRepo = new PttRedisRepository(mockRedis as unknown as Redis);
    roomRegistry = new PttRoomRegistry();
    controller = new PttController(redisRepo, roomRegistry);
  });

  afterEach(() => {
    roomRegistry.clearAll();
    vi.useRealTimers();
  });

  const createMockWs = (id: string, bufferedAmount = 0) => ({
    id,
    send: vi.fn(),
    close: vi.fn(),
    bufferedAmount,
    raw: { bufferedAmount },
  });

  describe('handleConnection', () => {
    it('harus meregistrasi soket, menambahkan ke Redis presence, mengirim state awal, dan membroadcast member count', async () => {
      mockRedis.scard.mockResolvedValueOnce(3);
      const ws = createMockWs('sock_conn_1');
      const user = { sub: 'usr_1', name: 'Budi', role: 'GUARD', group_uid: groupUid };

      await controller.handleConnection(ws, groupUid, user);

      expect(roomRegistry.getClientCount(groupUid)).toBe(1);
      expect(mockRedis.sadd).toHaveBeenCalledWith('ptt:room:group_cluster_102:members', 'usr_1');

      // State awal dikirim ke klien baru
      expect(ws.send).toHaveBeenCalledWith(
        JSON.stringify({
          type: 'FLOOR_STATE_CHANGED',
          state: 'IDLE',
          speaker: null,
          memberCount: 3,
        }),
      );
    });
  });

  describe('handleDisconnection', () => {
    it('harus merilis floor jika klien yang disconnect adalah active speaker', async () => {
      const ws = createMockWs('sock_speaker');
      const user = { sub: 'usr_speaker', name: 'Budi', role: 'GUARD', group_uid: groupUid };

      await controller.handleConnection(ws, groupUid, user);
      roomRegistry.setActiveSpeaker(groupUid, {
        socketId: 'sock_speaker',
        userId: 'usr_speaker',
        name: 'Budi',
        role: 'GUARD',
        burstStartMs: Date.now(),
      });

      mockRedis.eval.mockResolvedValueOnce(1); // releaseFloor success
      mockRedis.scard.mockResolvedValueOnce(0);

      await controller.handleDisconnection('sock_speaker', groupUid, user);

      expect(roomRegistry.getActiveSpeaker(groupUid)).toBeNull();
      expect(mockRedis.eval).toHaveBeenCalledWith(
        expect.stringContaining("redis.call('DEL', KEYS[1])"),
        1,
        'ptt:floor:group_cluster_102',
        'usr_speaker',
      );
    });
  });

  describe('Floor Request & Arbitration Hierarchy', () => {
    it('Guard berhasil mendapatkan floor saat IDLE, menerima FLOOR_GRANTED dan mengaktifkan 30s TOT', async () => {
      mockRedis.eval.mockResolvedValueOnce(JSON.stringify({ granted: true, preempted: false }));
      mockRedis.scard.mockResolvedValue(5);

      const ws = createMockWs('sock_guard_1');
      const user = { sub: 'usr_guard_1', name: 'Budi', role: 'GUARD', group_uid: groupUid };

      await controller.handleConnection(ws, groupUid, user);
      ws.send.mockClear();

      await controller.handleControlFrame(ws, groupUid, user, {
        type: 'FLOOR_REQUEST',
        requestId: 'req_101',
      });

      // 1. Requester menerima FLOOR_GRANTED
      expect(ws.send).toHaveBeenCalledWith(
        JSON.stringify({
          type: 'FLOOR_GRANTED',
          requestId: 'req_101',
          leaseTtlMs: 2000,
        }),
      );

      // 2. Active speaker tercatat di registry
      expect(roomRegistry.isCurrentActiveSpeaker(groupUid, 'sock_guard_1')).toBe(true);
    });

    it('Guard kedua ditolak saat floor sedang sibuk (FIFO), menerima FLOOR_DENIED', async () => {
      mockRedis.eval.mockResolvedValueOnce(
        JSON.stringify({
          granted: false,
          preempted: false,
          currentSpeaker: { userId: 'usr_guard_1', name: 'Budi', role: 'GUARD' },
        }),
      );

      const ws2 = createMockWs('sock_guard_2');
      const user2 = { sub: 'usr_guard_2', name: 'Agus', role: 'GUARD', group_uid: groupUid };

      await controller.handleConnection(ws2, groupUid, user2);
      ws2.send.mockClear();

      await controller.handleControlFrame(ws2, groupUid, user2, {
        type: 'FLOOR_REQUEST',
        requestId: 'req_102',
      });

      expect(ws2.send).toHaveBeenCalledWith(
        JSON.stringify({
          type: 'FLOOR_DENIED',
          requestId: 'req_102',
          reason: 'CHANNEL_BUSY',
          currentSpeaker: {
            userId: 'usr_guard_1',
            name: 'Budi',
            role: 'GUARD',
          },
        }),
      );
    });

    it('Admin berhasil melakukan Preemption terhadap Guard: Guard menerima FLOOR_REVOKED, listener menerima FLOOR_RESET', async () => {
      // 1. Setup active guard
      const wsGuard = createMockWs('sock_guard_1');
      const guardUser = { sub: 'usr_guard_1', name: 'Budi', role: 'GUARD', group_uid: groupUid };
      await controller.handleConnection(wsGuard, groupUid, guardUser);
      roomRegistry.setActiveSpeaker(groupUid, {
        socketId: 'sock_guard_1',
        userId: 'usr_guard_1',
        name: 'Budi',
        role: 'GUARD',
        burstStartMs: Date.now(),
      });

      // 2. Setup listening guard
      const wsListener = createMockWs('sock_listener');
      const listenerUser = {
        sub: 'usr_listener',
        name: 'Siti',
        role: 'GUARD',
        group_uid: groupUid,
      };
      await controller.handleConnection(wsListener, groupUid, listenerUser);

      // 3. Admin connects and requests floor
      const wsAdmin = createMockWs('sock_admin');
      const adminUser = {
        sub: 'usr_admin',
        name: 'Captain Hendra',
        role: 'ADMIN',
        group_uid: groupUid,
      };
      await controller.handleConnection(wsAdmin, groupUid, adminUser);

      mockRedis.eval.mockResolvedValueOnce(
        JSON.stringify({
          granted: true,
          preempted: true,
          previousSpeaker: { userId: 'usr_guard_1', name: 'Budi', role: 'GUARD' },
        }),
      );
      mockRedis.scard.mockResolvedValue(3);

      wsGuard.send.mockClear();
      wsListener.send.mockClear();
      wsAdmin.send.mockClear();

      await controller.handleControlFrame(wsAdmin, groupUid, adminUser, {
        type: 'FLOOR_REQUEST',
        requestId: 'req_admin_999',
      });

      // A. Admin menerima FLOOR_GRANTED
      expect(wsAdmin.send).toHaveBeenCalledWith(
        JSON.stringify({
          type: 'FLOOR_GRANTED',
          requestId: 'req_admin_999',
          leaseTtlMs: 2000,
        }),
      );

      // B. Guard yang dioverride menerima FLOOR_REVOKED (ADMIN_OVERRIDE)
      expect(wsGuard.send).toHaveBeenCalledWith(
        JSON.stringify({
          type: 'FLOOR_REVOKED',
          reason: 'ADMIN_OVERRIDE',
          preemptedBy: 'Captain Hendra',
        }),
      );

      // C. Listener menerima FLOOR_RESET (PREEMPTION_FLUSH) untuk membersihkan jitter buffer
      expect(wsListener.send).toHaveBeenCalledWith(
        JSON.stringify({
          type: 'FLOOR_RESET',
          reason: 'PREEMPTION_FLUSH',
        }),
      );

      // D. Active speaker kini adalah Admin
      expect(roomRegistry.isCurrentActiveSpeaker(groupUid, 'sock_admin')).toBe(true);
      expect(roomRegistry.isCurrentActiveSpeaker(groupUid, 'sock_guard_1')).toBe(false);
    });
  });

  describe('Floor Release', () => {
    it('Active speaker melepaskan tombol, membroadcast FLOOR_STATE_CHANGED (IDLE)', async () => {
      const ws = createMockWs('sock_speaker');
      const user = { sub: 'usr_speaker', name: 'Budi', role: 'GUARD', group_uid: groupUid };
      await controller.handleConnection(ws, groupUid, user);

      roomRegistry.setActiveSpeaker(groupUid, {
        socketId: 'sock_speaker',
        userId: 'usr_speaker',
        name: 'Budi',
        role: 'GUARD',
        burstStartMs: Date.now(),
      });

      mockRedis.eval.mockResolvedValueOnce(1); // releaseFloor success
      mockRedis.scard.mockResolvedValue(2);

      const wsListener = createMockWs('sock_listener');
      await controller.handleConnection(wsListener, groupUid, {
        sub: 'usr_2',
        name: 'Agus',
        role: 'GUARD',
      });
      wsListener.send.mockClear();

      await controller.handleControlFrame(ws, groupUid, user, {
        type: 'FLOOR_RELEASED',
        reason: 'BUTTON_RELEASED',
      });

      expect(roomRegistry.getActiveSpeaker(groupUid)).toBeNull();
      expect(wsListener.send).toHaveBeenCalledWith(
        JSON.stringify({
          type: 'FLOOR_STATE_CHANGED',
          state: 'IDLE',
          speaker: null,
          memberCount: 2,
        }),
      );
    });

    it('Klien yang BUKAN active speaker diabaikan jika mengirimkan FLOOR_RELEASED', async () => {
      const wsNonSpeaker = createMockWs('sock_other');
      const user = { sub: 'usr_other', name: 'Agus', role: 'GUARD', group_uid: groupUid };
      await controller.handleConnection(wsNonSpeaker, groupUid, user);

      await controller.handleControlFrame(wsNonSpeaker, groupUid, user, {
        type: 'FLOOR_RELEASED',
      });

      expect(mockRedis.eval).not.toHaveBeenCalledWith(
        expect.stringContaining("redis.call('DEL', KEYS[1])"),
        expect.anything(),
        expect.anything(),
        expect.anything(),
      );
    });
  });

  describe('Binary Audio & In-Flight Dropping Guard (MAJ-08)', () => {
    it('membuang (drop) paket binary audio jika pengirim BUKAN active speaker saat ini', () => {
      const wsSpeaker = createMockWs('sock_speaker');
      const wsRogue = createMockWs('sock_rogue');
      const wsListener = createMockWs('sock_listener');

      roomRegistry.register(groupUid, {
        id: 'sock_speaker',
        ws: wsSpeaker,
        userId: 'usr_speaker',
        name: 'Speaker',
        role: 'GUARD',
      });
      roomRegistry.register(groupUid, {
        id: 'sock_rogue',
        ws: wsRogue,
        userId: 'usr_rogue',
        name: 'Rogue',
        role: 'GUARD',
      });
      roomRegistry.register(groupUid, {
        id: 'sock_listener',
        ws: wsListener,
        userId: 'usr_listener',
        name: 'Listener',
        role: 'GUARD',
      });

      roomRegistry.setActiveSpeaker(groupUid, {
        socketId: 'sock_speaker',
        userId: 'usr_speaker',
        name: 'Speaker',
        role: 'GUARD',
        burstStartMs: Date.now(),
      });

      const audioBuffer = Buffer.from([0x10, 0x01, 0x02]);

      // Paket audio dari non-speaker harus dibuang seketika
      controller.handleBinaryAudioFrame('sock_rogue', groupUid, { sub: 'usr_rogue' }, audioBuffer);
      expect(wsListener.send).not.toHaveBeenCalled();

      // Paket audio dari active speaker sah diteruskan ke listener
      controller.handleBinaryAudioFrame(
        'sock_speaker',
        groupUid,
        { sub: 'usr_speaker' },
        audioBuffer,
      );
      expect(wsListener.send).toHaveBeenCalledWith(audioBuffer);
    });

    it('melakukan throttling 1Hz pada perpanjangan Redis lease agar tidak membombardir Redis pada 50Hz audio', () => {
      mockRedis.eval.mockResolvedValue(1);

      const wsSpeaker = createMockWs('sock_speaker');
      roomRegistry.register(groupUid, {
        id: 'sock_speaker',
        ws: wsSpeaker,
        userId: 'usr_speaker',
        name: 'Speaker',
        role: 'GUARD',
      });
      roomRegistry.setActiveSpeaker(groupUid, {
        socketId: 'sock_speaker',
        userId: 'usr_speaker',
        name: 'Speaker',
        role: 'GUARD',
        burstStartMs: Date.now(),
      });

      const audioBuffer = Buffer.from([0x10, 0x01]);

      // Kirim 10 paket audio dalam rentang 200ms (50Hz)
      for (let i = 0; i < 10; i++) {
        vi.advanceTimersByTime(20);
        controller.handleBinaryAudioFrame(
          'sock_speaker',
          groupUid,
          { sub: 'usr_speaker' },
          audioBuffer,
        );
      }

      // Redis extend lease hanya terpanggil 1 kali pada paket pertama, tidak 10 kali
      expect(mockRedis.eval).toHaveBeenCalledTimes(1);

      // Setelah 1,000ms berlalu
      vi.advanceTimersByTime(1001);
      controller.handleBinaryAudioFrame(
        'sock_speaker',
        groupUid,
        { sub: 'usr_speaker' },
        audioBuffer,
      );

      // Terpanggil untuk kedua kalinya setelah window 1 detik
      expect(mockRedis.eval).toHaveBeenCalledTimes(2);
    });
  });

  describe('Maximum Transmission Time-Out Timer (30s TOT) (MAJ-02)', () => {
    it('memutus paksa transmisi saat mencapai batas 30 detik: mengirim FLOOR_REVOKED ke speaker dan mereset floor ke IDLE', async () => {
      mockRedis.eval.mockResolvedValueOnce(JSON.stringify({ granted: true, preempted: false }));
      mockRedis.scard.mockResolvedValue(2);

      const wsSpeaker = createMockWs('sock_speaker');
      const wsListener = createMockWs('sock_listener');

      await controller.handleConnection(wsSpeaker, groupUid, {
        sub: 'usr_speaker',
        name: 'Budi',
        role: 'GUARD',
      });
      await controller.handleConnection(wsListener, groupUid, {
        sub: 'usr_listener',
        name: 'Agus',
        role: 'GUARD',
      });

      // Grant floor
      await controller.handleControlFrame(
        wsSpeaker,
        groupUid,
        { sub: 'usr_speaker', name: 'Budi', role: 'GUARD' },
        { type: 'FLOOR_REQUEST', requestId: 'req_tot' },
      );

      wsSpeaker.send.mockClear();
      wsListener.send.mockClear();
      mockRedis.eval.mockResolvedValueOnce(1); // releaseFloor

      // Majukan waktu 30 detik secara asynchronous
      await vi.advanceTimersByTimeAsync(30000);

      // 1. Speaker menerima FLOOR_REVOKED (TIME_OUT_TIMER_EXPIRED)
      expect(wsSpeaker.send).toHaveBeenCalledWith(
        JSON.stringify({
          type: 'FLOOR_REVOKED',
          reason: 'TIME_OUT_TIMER_EXPIRED',
        }),
      );

      // 2. Seluruh subscriber menerima FLOOR_STATE_CHANGED (IDLE)
      expect(wsListener.send).toHaveBeenCalledWith(
        JSON.stringify({
          type: 'FLOOR_STATE_CHANGED',
          state: 'IDLE',
          speaker: null,
          memberCount: 2,
        }),
      );

      // 3. Active speaker dibersihkan
      expect(roomRegistry.getActiveSpeaker(groupUid)).toBeNull();
    });
  });
});
