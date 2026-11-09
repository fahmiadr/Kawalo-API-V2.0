import { logger } from '@core/utils/logger.service';
import type {
  IncidentEvent,
  IncidentEventPublisher,
} from '@features/incident/domain/repositories/incident.repository';

/** The data is already saved when the event is sent, so a pub/sub failure must not fail the request. */
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
