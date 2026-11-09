import { redis } from '@core/database/redis.service';
import { logger } from '@core/utils/logger.service';
import type {
  IncidentEvent,
  IncidentEventPublisher,
} from '@features/incident/domain/repositories/incident.repository';
import type Redis from 'ioredis';

type Listener = (event: IncidentEvent) => void;

const channel = (incidentId: string) => `incident:${incidentId}:events`;

/**
 * Pub/sub Redis: event tersebar ke semua instance server, jadi klien SSE yang
 * tersambung ke instance lain tetap menerima balasan.
 * Satu koneksi subscriber bersama; subscribe/unsubscribe per channel saat listener pertama/terakhir.
 */
export class IncidentEventBus implements IncidentEventPublisher {
  private subscriber: Redis | null = null;
  private readonly listeners = new Map<string, Set<Listener>>();

  public async publish(incidentId: string, event: IncidentEvent): Promise<void> {
    await redis.publish(channel(incidentId), JSON.stringify(event));
  }

  /** Mengembalikan fungsi unsubscribe. */
  public async subscribe(incidentId: string, listener: Listener): Promise<() => Promise<void>> {
    const sub = this.getSubscriber();
    const ch = channel(incidentId);
    let set = this.listeners.get(ch);
    if (!set) {
      set = new Set();
      this.listeners.set(ch, set);
      await sub.subscribe(ch);
    }
    set.add(listener);

    return async () => {
      const current = this.listeners.get(ch);
      if (!current) return;
      current.delete(listener);
      if (current.size === 0) {
        this.listeners.delete(ch);
        await sub.unsubscribe(ch).catch(() => undefined);
      }
    };
  }

  private getSubscriber(): Redis {
    if (!this.subscriber) {
      this.subscriber = redis.duplicate();
      this.subscriber.on('message', (ch: string, message: string) => {
        const set = this.listeners.get(ch);
        if (!set) return;
        try {
          const event = JSON.parse(message) as IncidentEvent;
          for (const listener of set) listener(event);
        } catch (err) {
          logger.warn({ err }, '[Incident] Event pub/sub tidak valid diabaikan');
        }
      });
      this.subscriber.on('error', (err) =>
        logger.error({ err }, '[Incident] Redis subscriber error'),
      );
    }
    return this.subscriber;
  }
}
