import {
  type PttClientSocket,
  PttRoomRegistry,
} from '@features/ptt/infrastructure/ptt-room-registry';
import { beforeEach, describe, expect, it, vi } from 'vitest';

describe('PttRoomRegistry Unit Tests', () => {
  let registry: PttRoomRegistry;
  const groupUid = 'group_cluster_102';

  beforeEach(() => {
    registry = new PttRoomRegistry();
  });

  const createMockSocket = (
    id: string,
    userId: string,
    role = 'GUARD',
    bufferedAmount = 0,
  ): PttClientSocket => ({
    id,
    userId,
    name: `User ${userId}`,
    role,
    ws: {
      send: vi.fn(),
      close: vi.fn(),
      bufferedAmount,
      raw: { bufferedAmount },
    },
  });

  it('harus mendaftarkan (register) dan membatalkan (unregister) klien soket dengan benar', () => {
    const client1 = createMockSocket('sock_1', 'usr_1', 'GUARD');
    const client2 = createMockSocket('sock_2', 'usr_2', 'GUARD');

    registry.register(groupUid, client1);
    registry.register(groupUid, client2);

    expect(registry.getClientCount(groupUid)).toBe(2);
    expect(registry.getClient(groupUid, 'sock_1')).toBe(client1);
    expect(registry.getClientByUserId(groupUid, 'usr_2')).toBe(client2);

    const { remainingCount, removedClient } = registry.unregister(groupUid, 'sock_1');
    expect(remainingCount).toBe(1);
    expect(removedClient).toBe(client1);
    expect(registry.getClient(groupUid, 'sock_1')).toBeUndefined();
    expect(registry.getClientCount(groupUid)).toBe(1);
  });

  it('harus membersihkan metadata room saat seluruh klien telah disconnect', () => {
    const client = createMockSocket('sock_1', 'usr_1');
    registry.register(groupUid, client);
    registry.setActiveSpeaker(groupUid, {
      socketId: 'sock_1',
      userId: 'usr_1',
      name: 'User 1',
      role: 'GUARD',
      burstStartMs: Date.now(),
    });

    const { remainingCount } = registry.unregister(groupUid, 'sock_1');
    expect(remainingCount).toBe(0);
    expect(registry.getActiveSpeaker(groupUid)).toBeNull();
  });

  describe('broadcastJson', () => {
    it('harus membroadcast JSON frame ke seluruh subscriber kecuali sender jika dikecualikan', () => {
      const client1 = createMockSocket('sock_1', 'usr_1');
      const client2 = createMockSocket('sock_2', 'usr_2');
      const client3 = createMockSocket('sock_3', 'usr_3');

      registry.register(groupUid, client1);
      registry.register(groupUid, client2);
      registry.register(groupUid, client3);

      const payload = { type: 'FLOOR_STATE_CHANGED', state: 'IDLE' };
      registry.broadcastJson(groupUid, payload, 'sock_1');

      expect(client1.ws.send).not.toHaveBeenCalled();
      expect(client2.ws.send).toHaveBeenCalledWith(JSON.stringify(payload));
      expect(client3.ws.send).toHaveBeenCalledWith(JSON.stringify(payload));
    });
  });

  describe('broadcastBinary with TCP Backpressure Drop Guard (MAJ-04)', () => {
    it('harus mengirim paket binary audio ke subscriber dengan buffer normal dan membuang (drop) untuk subscriber lambat', () => {
      // client1 = sender
      const client1 = createMockSocket('sock_1', 'usr_1', 'GUARD', 0);
      // client2 = normal connection (bufferedAmount = 512B)
      const client2 = createMockSocket('sock_2', 'usr_2', 'GUARD', 512);
      // client3 = stalled cellular connection (bufferedAmount = 12KB > 5KB threshold)
      const client3 = createMockSocket('sock_3', 'usr_3', 'GUARD', 12 * 1024);

      registry.register(groupUid, client1);
      registry.register(groupUid, client2);
      registry.register(groupUid, client3);

      const audioBuffer = Buffer.from([0x10, 0x00, 0x01, 0x00, 0x00, 0x00, 0x20]);
      const result = registry.broadcastBinary(groupUid, audioBuffer, 'sock_1');

      expect(result.sentCount).toBe(1);
      expect(result.droppedCount).toBe(1);

      expect(client1.ws.send).not.toHaveBeenCalled();
      expect(client2.ws.send).toHaveBeenCalledWith(audioBuffer);
      // Client 3 should NOT receive packet to prevent memory bloat and late chipmunk audio burst
      expect(client3.ws.send).not.toHaveBeenCalled();
    });
  });

  describe('Active Speaker & Preemption In-Flight Guards', () => {
    it('harus memvalidasi apakah socketId tertentu adalah active speaker saat ini', () => {
      registry.setActiveSpeaker(groupUid, {
        socketId: 'sock_active',
        userId: 'usr_speaker',
        name: 'Speaker',
        role: 'GUARD',
        burstStartMs: Date.now(),
      });

      expect(registry.isCurrentActiveSpeaker(groupUid, 'sock_active')).toBe(true);
      expect(registry.isCurrentActiveSpeaker(groupUid, 'sock_other')).toBe(false);

      registry.setActiveSpeaker(groupUid, null);
      expect(registry.isCurrentActiveSpeaker(groupUid, 'sock_active')).toBe(false);
    });
  });

  describe('Time-Out Timer (30s TOT) Management', () => {
    it('harus mengatur dan membatalkan TOT timer dengan benar', () => {
      vi.useFakeTimers();
      const mockTimerCallback = vi.fn();
      const timer = setTimeout(mockTimerCallback, 30000);

      registry.setTotTimer(groupUid, timer);
      registry.clearTotTimer(groupUid);

      vi.advanceTimersByTime(35000);
      expect(mockTimerCallback).not.toHaveBeenCalled();
      vi.useRealTimers();
    });
  });

  describe('Lease Renewal Throttling (1Hz)', () => {
    it('harus mencatat dan memperbarui timestamp lease renewal per group', () => {
      expect(registry.getLastLeaseRenewal(groupUid)).toBe(0);

      const now = 1700000000000;
      registry.setLastLeaseRenewal(groupUid, now);
      expect(registry.getLastLeaseRenewal(groupUid)).toBe(now);
    });
  });
});
