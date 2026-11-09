import { randomBytes } from 'node:crypto';
import { redis } from '@core/database/redis.service';
import { logger } from '@core/utils/logger.service';
import {
  type PttRoomRegistry,
  pttRoomRegistry,
} from '@features/ptt/infrastructure/ptt-room-registry';
import type Redis from 'ioredis';

/** The subset of ioredis the relay uses (keeps it easy to fake in unit tests). */
export type PttPubSubClient = Pick<Redis, 'publish' | 'subscribe' | 'unsubscribe' | 'on'>;

const ORIGIN_LEN = 16; // length of the instanceId (hex ASCII) prefixed to each audio frame

type ControlMessage =
  | { o: string; t: 'json'; payload: string; exclude?: string }
  | { o: string; t: 'preempted'; previousUserId: string; frame: unknown };

/**
 * Cross-instance relay for PTT over Redis pub/sub.
 *
 * The socket registry lives in one process's memory, so without a relay squad members connected
 * to another instance would receive neither audio nor control frames. Design:
 * - Local fan-out stays direct (zero latency); the relay only forwards to OTHER instances.
 * - Every message carries the sender's `instanceId`; receivers drop their own messages.
 * - Two channels per group: `ctl` (JSON: control frames, preempt signal) and `audio` (binary Opus).
 * - A group is subscribed only while at least one local socket is in it (join/leave).
 *
 * ponytail: audio is always published (~50 msg/s per active speaker) even with no other instance.
 * If Redis becomes the bottleneck, cache `PUBSUB NUMSUB` per group and skip publishing without peers.
 * The floor lease in Redis (5s TTL) covers a crashed instance; listeners on other instances only
 * return to IDLE on the next FLOOR_STATE_CHANGED.
 */
export class PttClusterRelay {
  public readonly instanceId = randomBytes(ORIGIN_LEN / 2).toString('hex');
  private subscriber: PttPubSubClient | null = null;
  private readonly joined = new Set<string>();

  constructor(
    private readonly registry: PttRoomRegistry = pttRoomRegistry,
    private readonly publisher: PttPubSubClient = redis,
    private readonly createSubscriber: () => PttPubSubClient = () => redis.duplicate(),
  ) {}

  private static ctl = (groupUid: string) => `ptt:room:${groupUid}:ctl`;
  private static audio = (groupUid: string) => `ptt:room:${groupUid}:audio`;

  public async join(groupUid: string): Promise<void> {
    if (this.joined.has(groupUid)) return;
    this.joined.add(groupUid);
    await this.getSubscriber().subscribe(
      PttClusterRelay.ctl(groupUid),
      PttClusterRelay.audio(groupUid),
    );
  }

  public async leave(groupUid: string): Promise<void> {
    if (!this.joined.delete(groupUid)) return;
    await this.getSubscriber()
      .unsubscribe(PttClusterRelay.ctl(groupUid), PttClusterRelay.audio(groupUid))
      .catch(() => undefined);
  }

  /** Forwards a JSON frame to other instances (the caller does the local fan-out). */
  public publishJson(groupUid: string, payload: unknown, excludeSocketId?: string): void {
    this.sendCtl(groupUid, {
      o: this.instanceId,
      t: 'json',
      payload: typeof payload === 'string' ? payload : JSON.stringify(payload),
      exclude: excludeSocketId,
    });
  }

  /** Tells other instances the previous speaker was preempted so they stop relaying that audio. */
  public publishPreempted(groupUid: string, previousUserId: string, frame: unknown): void {
    this.sendCtl(groupUid, { o: this.instanceId, t: 'preempted', previousUserId, frame });
  }

  public publishBinary(groupUid: string, frame: Buffer | Uint8Array): void {
    const message = Buffer.concat([Buffer.from(this.instanceId, 'ascii'), frame]);
    this.publisher.publish(PttClusterRelay.audio(groupUid), message).catch(this.logFailure);
  }

  private sendCtl(groupUid: string, message: ControlMessage): void {
    this.publisher
      .publish(PttClusterRelay.ctl(groupUid), JSON.stringify(message))
      .catch(this.logFailure);
  }

  private readonly logFailure = (err: unknown) =>
    logger.warn({ err }, '[PTT] Gagal publish ke relay cluster');

  private getSubscriber(): PttPubSubClient {
    if (!this.subscriber) {
      this.subscriber = this.createSubscriber();
      this.subscriber.on('messageBuffer', (channel: Buffer, message: Buffer) =>
        this.onMessage(channel.toString(), message),
      );
    }
    return this.subscriber;
  }

  private onMessage(channel: string, message: Buffer): void {
    const match = /^ptt:room:(.+):(ctl|audio)$/.exec(channel);
    const groupUid = match?.[1];
    if (!groupUid) return;

    if (match[2] === 'audio') {
      if (message.subarray(0, ORIGIN_LEN).toString('ascii') === this.instanceId) return;
      this.registry.broadcastBinary(groupUid, message.subarray(ORIGIN_LEN));
      return;
    }

    let data: ControlMessage;
    try {
      data = JSON.parse(message.toString()) as ControlMessage;
    } catch {
      return;
    }
    if (data.o === this.instanceId) return;

    if (data.t === 'json') {
      this.registry.broadcastJson(groupUid, data.payload, data.exclude);
      return;
    }

    // preempted: only the instance holding the previous speaker's socket acts
    const active = this.registry.getActiveSpeaker(groupUid);
    if (active?.userId !== data.previousUserId) return;
    this.registry.clearTotTimer(groupUid);
    this.registry.setActiveSpeaker(groupUid, null);
    try {
      this.registry.getClient(groupUid, active.socketId)?.ws.send(JSON.stringify(data.frame));
    } catch {
      // socket already closed
    }
  }
}

export const pttClusterRelay = new PttClusterRelay();
