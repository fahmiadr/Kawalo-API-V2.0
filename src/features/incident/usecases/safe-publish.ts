import { logger } from '@core/utils/logger.service';
import type {
  IncidentEvent,
  IncidentEventPublisher,
} from '@features/incident/domain/repositories/incident.repository';

/** Data sudah tersimpan saat event dikirim; kegagalan pub/sub tidak boleh menggagalkan request. */
export async function safePublish(
  publisher: IncidentEventPublisher,
  incidentId: string,
  event: IncidentEvent,
): Promise<void> {
  try {
    await publisher.publish(incidentId, event);
  } catch (err) {
    logger.warn({ err, incidentId, type: event.type }, '[Incident] Gagal mempublikasikan event');
  }
}
