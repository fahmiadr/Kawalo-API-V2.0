import { randomBytes } from 'node:crypto';
import { redis } from '@core/database/redis.service';
import { logger } from '@core/utils/logger.service';
import {
  type PttRoomRegistry,
  pttRoomRegistry,
} from '@features/ptt/infrastructure/ptt-room-registry';
import type Redis from 'ioredis';

/** Subset ioredis yang dipakai relay (memudahkan fake di unit test). */
export type PttPubSubClient = Pick<Redis, 'publish' | 'subscribe' | 'unsubscribe' | 'on'>;

const ORIGIN_LEN = 16; // panjang instanceId (hex ASCII) di awal frame audio

type ControlMessage =
  | { o: string; t: 'json'; payload: string; exclude?: string }
  | { o: string; t: 'preempted'; previousUserId: string; frame: unknown };

/**
 * Relay antar-instance untuk PTT lewat Redis pub/sub.
 *
 * Registry soket hanya hidup di memori satu proses, jadi tanpa relay anggota squad yang
 * tersambung ke instance lain tidak menerima audio/frame kontrol. Pola:
 * - Fan-out lokal tetap langsung (latensi nol); relay hanya meneruskan ke instance LAIN.
 * - Tiap pesan diberi `instanceId` pengirim; penerima membuang pesan miliknya sendiri.
 * - Dua channel per grup: `ctl` (JSON: frame kontrol, sinyal preempt) dan `audio` (biner Opus).
 * - Subscribe hanya selama ada soket lokal di grup tersebut (join/leave).
 *
 * ponytail: audio dipublish selalu (~50 msg/s per pembicara aktif) walau tak ada instance lain.
 * Bila Redis jadi bottleneck, cache `PUBSUB NUMSUB` per grup dan lewati publish saat tak ada peer.
 * Lease lantai di Redis (TTL 5 dtk) menutup kasus instance mati; listener di instance lain baru
 * kembali IDLE saat ada FLOOR_STATE_CHANGED berikutnya.
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

  /** Teruskan frame JSON ke instance lain (fan-out lokal dilakukan pemanggil). */
  public publishJson(groupUid: string, payload: unknown, excludeSocketId?: string): void {
    this.sendCtl(groupUid, {
      o: this.instanceId,
      t: 'json',
      payload: typeof payload === 'string' ? payload : JSON.stringify(payload),
      exclude: excludeSocketId,
    });
  }

  /** Beri tahu instance lain bahwa pembicara sebelumnya dipreempt (agar audionya dihentikan di sana). */
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

    // preempted: hanya instance yang memegang soket pembicara sebelumnya yang bertindak
    const active = this.registry.getActiveSpeaker(groupUid);
    if (active?.userId !== data.previousUserId) return;
    this.registry.clearTotTimer(groupUid);
    this.registry.setActiveSpeaker(groupUid, null);
    try {
      this.registry.getClient(groupUid, active.socketId)?.ws.send(JSON.stringify(data.frame));
    } catch {
      // soket sudah tertutup
    }
  }
}

export const pttClusterRelay = new PttClusterRelay();
