import type { IncidentEvent } from '@features/incident/domain/repositories/incident.repository';
import { IncidentMapper } from '@features/incident/presentation/mappers/incident.mapper';

export interface IncidentEventSource {
  subscribe(
    incidentId: string,
    listener: (event: IncidentEvent) => void,
  ): Promise<() => Promise<void>>;
}

const HEARTBEAT_MS = 15_000; // app memutus koneksi bila senyap > 30 detik

/**
 * Stream SSE untuk satu thread insiden.
 * - Subscribe dulu, baru kirim `sync_required` (bila reconnect) agar tidak ada event yang terlewat.
 * - Reconnect dengan Last-Event-ID -> `sync_required`: app mengambil ulang detail (menangani
 *   balasan, resolve, dan delete yang terlewat sekaligus) tanpa buffer replay di server.
 */
export function createIncidentSseStream(opts: {
  incidentId: string;
  lastEventId?: string;
  source: IncidentEventSource;
  signal?: AbortSignal;
  heartbeatMs?: number;
}): ReadableStream<Uint8Array> {
  const encoder = new TextEncoder();
  let cleanup: (() => Promise<void>) | undefined;

  return new ReadableStream<Uint8Array>({
    async start(controller) {
      const send = (chunk: string) => {
        try {
          controller.enqueue(encoder.encode(chunk));
        } catch {
          // stream sudah ditutup
        }
      };

      const unsubscribe = await opts.source.subscribe(opts.incidentId, (event) => {
        const frame = IncidentMapper.toSseFrame(event);
        send(
          `${frame.id ? `id: ${frame.id}\n` : ''}event: ${frame.name}\ndata: ${JSON.stringify(frame.data)}\n\n`,
        );
      });
      const timer = setInterval(() => send(': ping\n\n'), opts.heartbeatMs ?? HEARTBEAT_MS);

      let closed = false;
      cleanup = async () => {
        if (closed) return;
        closed = true;
        clearInterval(timer);
        await unsubscribe();
      };
      opts.signal?.addEventListener('abort', () => {
        void cleanup?.();
        try {
          controller.close();
        } catch {
          // sudah tertutup
        }
      });

      send('retry: 3000\n\n');
      if (opts.lastEventId) send('event: sync_required\ndata: {}\n\n');
    },
    async cancel() {
      await cleanup?.();
    },
  });
}
