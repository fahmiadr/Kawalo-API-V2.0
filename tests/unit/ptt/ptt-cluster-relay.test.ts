import {
  PttClusterRelay,
  type PttPubSubClient,
} from '@features/ptt/infrastructure/ptt-cluster-relay';
import { PttRedisRepository } from '@features/ptt/infrastructure/ptt-redis.repository';
import { PttRoomRegistry } from '@features/ptt/infrastructure/ptt-room-registry';
import { PttController } from '@features/ptt/presentation/ptt.controller';
import type Redis from 'ioredis';
import { afterEach, describe, expect, it, vi } from 'vitest';

type Listener = (channel: Buffer, message: Buffer) => void;

/** In-memory Redis pub/sub: one hub shared by several "instances". */
class FakeHub {
  private subs = new Set<{ channels: Set<string>; listener?: Listener }>();

  publisher = {
    publish: async (channel: string, data: string | Buffer) => {
      for (const s of this.subs) {
        if (s.channels.has(channel)) s.listener?.(Buffer.from(channel), Buffer.from(data));
      }
      return 1;
    },
  } as unknown as PttPubSubClient;

  createSubscriber = (): PttPubSubClient => {
    const sub: { channels: Set<string>; listener?: Listener } = { channels: new Set() };
    this.subs.add(sub);
    return {
      subscribe: async (...chs: string[]) => {
        for (const c of chs) sub.channels.add(c);
      },
      unsubscribe: async (...chs: string[]) => {
        for (const c of chs) sub.channels.delete(c);
      },
      on: (_event: string, l: Listener) => {
        sub.listener = l;
      },
    } as unknown as PttPubSubClient;
  };

  subscriptionCount = () => [...this.subs].reduce((n, s) => n + s.channels.size, 0);
}

const group = 'g_cluster';
const flush = () => new Promise((r) => setTimeout(r, 0));
const ws = (id: string) => ({ id, send: vi.fn(), bufferedAmount: 0, raw: { bufferedAmount: 0 } });
const sent = (w: ReturnType<typeof ws>) =>
  w.send.mock.calls.map((c) => (typeof c[0] === 'string' ? JSON.parse(c[0]) : c[0]));

const registries: PttRoomRegistry[] = [];
afterEach(() => {
  for (const r of registries) r.clearAll(); // stops the 30s TOT timers
  registries.length = 0;
});

/** One "server instance": registry + repo (Redis mocked) + relay on the shared hub. */
function makeInstance(
  hub: FakeHub,
  acquire: () => object = () => ({ granted: true, preempted: false }),
) {
  const registry = new PttRoomRegistry();
  registries.push(registry);
  const redisMock = {
    eval: vi.fn(async (_s: string, _n: number, _k: string, ...args: unknown[]) =>
      args.length >= 3 ? JSON.stringify(acquire()) : 1,
    ),
    get: vi.fn().mockResolvedValue(null),
    del: vi.fn().mockResolvedValue(1),
    sadd: vi.fn().mockResolvedValue(1),
    srem: vi.fn().mockResolvedValue(1),
    scard: vi.fn().mockResolvedValue(2),
    expire: vi.fn().mockResolvedValue(1),
  };
  const repo = new PttRedisRepository(redisMock as unknown as Redis);
  const relay = new PttClusterRelay(registry, hub.publisher, hub.createSubscriber);
  return { registry, relay, controller: new PttController(repo, registry, relay) };
}

const guard = { sub: 'u_guard', name: 'Budi', role: 'GUARD', group_uid: group };
const admin = { sub: 'u_admin', name: 'Hendra', role: 'ADMIN', group_uid: group };
const listener = { sub: 'u_listener', name: 'Sari', role: 'GUARD', group_uid: group };

describe('PttClusterRelay (multi-instance)', () => {
  it('frame kontrol dan audio sampai ke pendengar di instance lain, tepat sekali', async () => {
    const hub = new FakeHub();
    const A = makeInstance(hub);
    const B = makeInstance(hub);
    const speakerWs = ws('s_a');
    const localListenerWs = ws('l_a');
    const remoteListenerWs = ws('l_b');
    await A.controller.handleConnection(speakerWs, group, guard);
    await A.controller.handleConnection(localListenerWs, group, listener);
    await B.controller.handleConnection(remoteListenerWs, group, { ...listener, sub: 'u_l2' });
    await flush();

    await A.controller.handleControlFrame(speakerWs, group, guard, {
      type: 'FLOOR_REQUEST',
      requestId: 'r1',
    });
    await flush();
    const transmitting = (w: ReturnType<typeof ws>) =>
      sent(w).filter((f) => f?.type === 'FLOOR_STATE_CHANGED' && f.state === 'TRANSMITTING');
    expect(transmitting(remoteListenerWs)).toHaveLength(1);
    expect(transmitting(localListenerWs)).toHaveLength(1);

    const audio = Buffer.from([0x10, 1, 2, 3, 4, 5, 6, 7, 8, 9]);
    A.controller.handleBinaryAudioFrame('s_a', group, guard, audio);
    await flush();
    const audioOf = (w: ReturnType<typeof ws>) => sent(w).filter((f) => Buffer.isBuffer(f));
    expect(audioOf(remoteListenerWs)).toHaveLength(1);
    expect(Buffer.compare(audioOf(remoteListenerWs)[0], audio)).toBe(0);
    expect(audioOf(localListenerWs)).toHaveLength(1); // not duplicated: the relay drops its own messages
    expect(audioOf(speakerWs)).toHaveLength(0);
  });

  it('admin di instance B mempreempt guard di instance A: A mencabut lantai & menghentikan audio', async () => {
    const hub = new FakeHub();
    const A = makeInstance(hub);
    const B = makeInstance(hub, () => ({
      granted: true,
      preempted: true,
      previousSpeaker: { userId: guard.sub, name: guard.name, role: 'GUARD' },
    }));
    const guardWs = ws('s_guard');
    const adminWs = ws('s_admin');
    const listenerWs = ws('l_b');
    await A.controller.handleConnection(guardWs, group, guard);
    await B.controller.handleConnection(adminWs, group, admin);
    await B.controller.handleConnection(listenerWs, group, listener);
    await A.controller.handleControlFrame(guardWs, group, guard, {
      type: 'FLOOR_REQUEST',
      requestId: 'g1',
    });
    await flush();
    expect(A.registry.getActiveSpeaker(group)?.userId).toBe(guard.sub);

    await B.controller.handleControlFrame(adminWs, group, admin, {
      type: 'FLOOR_REQUEST',
      requestId: 'a1',
    });
    await flush();

    expect(sent(guardWs)).toContainEqual({
      type: 'FLOOR_REVOKED',
      reason: 'ADMIN_OVERRIDE',
      preemptedBy: 'Hendra',
    });
    expect(A.registry.getActiveSpeaker(group)).toBeNull();

    // late audio from the guard is dropped and not forwarded to instance B
    A.controller.handleBinaryAudioFrame(
      's_guard',
      group,
      guard,
      Buffer.from([0x10, 1, 2, 3, 4, 5, 6, 7]),
    );
    await flush();
    expect(sent(listenerWs).filter((f) => Buffer.isBuffer(f))).toHaveLength(0);
  });

  it('unsubscribe saat soket lokal terakhir keluar; subscribe lagi saat ada yang masuk', async () => {
    const hub = new FakeHub();
    const A = makeInstance(hub);
    const w = ws('s1');
    await A.controller.handleConnection(w, group, guard);
    expect(hub.subscriptionCount()).toBe(2); // ctl and audio channels
    await A.controller.handleDisconnection('s1', group, guard);
    expect(hub.subscriptionCount()).toBe(0);
    await A.controller.handleConnection(ws('s2'), group, guard);
    expect(hub.subscriptionCount()).toBe(2);
  });

  it('kegagalan publish tidak melempar ke jalur audio', async () => {
    const registry = new PttRoomRegistry();
    registries.push(registry);
    const failing = {
      publish: vi.fn().mockRejectedValue(new Error('redis down')),
    } as unknown as PttPubSubClient;
    const relay = new PttClusterRelay(registry, failing, new FakeHub().createSubscriber);
    expect(() => relay.publishBinary(group, Buffer.from([1, 2, 3]))).not.toThrow();
    await flush();
  });
});
