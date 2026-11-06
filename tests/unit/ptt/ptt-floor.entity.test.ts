import { PttFloor } from '@features/ptt/domain/ptt-floor.entity';
import { describe, expect, it } from 'vitest';

describe('PttFloor Domain Entity Unit Tests', () => {
  const groupUid = 'group_cluster_102';

  it('harus membuat floor berstatus IDLE dengan benar', () => {
    const floor = PttFloor.createIdle(groupUid);

    expect(floor.groupUid).toBe(groupUid);
    expect(floor.state).toBe('IDLE');
    expect(floor.speaker).toBeNull();
    expect(floor.isOccupied()).toBe(false);
    expect(floor.isExpired()).toBe(false);
    expect(floor.isTotExceeded()).toBe(false);
  });

  it('harus membuat floor berstatus TRANSMITTING dengan metadata speaker lengkap', () => {
    const now = 1700000000000;
    const speaker = {
      userId: 'usr_guard_1',
      name: 'Budi Santoso',
      role: 'GUARD',
    };

    const floor = PttFloor.createTransmitting(groupUid, speaker, now, 2000);

    expect(floor.state).toBe('TRANSMITTING');
    expect(floor.isOccupied()).toBe(true);
    expect(floor.speaker).toEqual(speaker);
    expect(floor.grantedAtMs).toBe(now);
    expect(floor.leaseExpiresAtMs).toBe(now + 2000);
    expect(floor.burstStartMs).toBe(now);
  });

  describe('isExpired & isTotExceeded', () => {
    it('harus mendeteksi masa sewa floor (lease) kadaluarsa setelah 2,000ms', () => {
      const now = 1700000000000;
      const floor = PttFloor.createTransmitting(
        groupUid,
        { userId: 'usr_1', name: 'Guard A', role: 'GUARD' },
        now,
        2000,
      );

      expect(floor.isExpired(now + 1999)).toBe(false);
      expect(floor.isExpired(now + 2000)).toBe(true);
      expect(floor.isExpired(now + 3000)).toBe(true);
    });

    it('harus mendeteksi Time-Out Timer (TOT) 30 detik terlampaui', () => {
      const now = 1700000000000;
      const floor = PttFloor.createTransmitting(
        groupUid,
        { userId: 'usr_1', name: 'Guard A', role: 'GUARD' },
        now,
        2000,
      );

      expect(floor.isTotExceeded(now + 29999)).toBe(false);
      expect(floor.isTotExceeded(now + 30000)).toBe(true);
      expect(floor.isTotExceeded(now + 45000)).toBe(true);
    });
  });

  describe('Two-Tier Arbitration & Preemption Hierarchy', () => {
    it('Admin dapat melakukan preemption terhadap transmisi Guard yang sedang aktif', () => {
      const floor = PttFloor.createTransmitting(groupUid, {
        userId: 'usr_guard_1',
        name: 'Guard A',
        role: 'GUARD',
      });

      expect(floor.canPreempt('ADMIN')).toBe(true);
      expect(floor.canPreempt('admin')).toBe(true);
    });

    it('Guard TIDAK DAPAT melakukan preemption terhadap sesama Guard (FIFO)', () => {
      const floor = PttFloor.createTransmitting(groupUid, {
        userId: 'usr_guard_1',
        name: 'Guard A',
        role: 'GUARD',
      });

      expect(floor.canPreempt('GUARD')).toBe(false);
      expect(floor.canPreempt('guard')).toBe(false);
    });

    it('Guard TIDAK DAPAT melakukan preemption terhadap Admin', () => {
      const floor = PttFloor.createTransmitting(groupUid, {
        userId: 'usr_admin_1',
        name: 'Captain Hendra',
        role: 'ADMIN',
      });

      expect(floor.canPreempt('GUARD')).toBe(false);
    });

    it('Admin TIDAK DAPAT melakukan preemption terhadap sesama Admin (FIFO antar Admin)', () => {
      const floor = PttFloor.createTransmitting(groupUid, {
        userId: 'usr_admin_1',
        name: 'Captain Hendra',
        role: 'ADMIN',
      });

      expect(floor.canPreempt('ADMIN')).toBe(false);
    });

    it('Floor yang IDLE selalu mengizinkan perolehan hak bicara oleh role manapun', () => {
      const floor = PttFloor.createIdle(groupUid);

      expect(floor.canPreempt('GUARD')).toBe(true);
      expect(floor.canPreempt('ADMIN')).toBe(true);
    });
  });

  it('harus memperbarui masa sewa (renewLease) dengan benar', () => {
    const now = 1700000000000;
    const floor = PttFloor.createTransmitting(
      groupUid,
      { userId: 'usr_1', name: 'Guard A', role: 'GUARD' },
      now,
      2000,
    );

    const renewed = floor.renewLease(now + 1000, 2000);
    expect(renewed.leaseExpiresAtMs).toBe(now + 3000);
    expect(renewed.burstStartMs).toBe(now); // burst start tetap konstan
  });

  it('harus merilis floor kembali ke status IDLE', () => {
    const floor = PttFloor.createTransmitting(groupUid, {
      userId: 'usr_1',
      name: 'Guard A',
      role: 'GUARD',
    });

    const released = floor.release();
    expect(released.state).toBe('IDLE');
    expect(released.speaker).toBeNull();
    expect(released.isOccupied()).toBe(false);
  });
});
