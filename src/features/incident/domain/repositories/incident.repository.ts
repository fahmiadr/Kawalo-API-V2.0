import type {
  IncidentEntity,
  IncidentReplyEntity,
  IncidentStatus,
} from '@features/incident/domain/entities/incident.entity';

export interface IncidentFilterParams {
  status?: IncidentStatus;
  search?: string;
  startDate?: string;
  endDate?: string;
  limit?: number;
  offset?: number;
}

export interface NewIncident {
  id: string;
  groupUid: string;
  title: string;
  description: string | null;
  category: string;
  glat: number | null;
  glong: number | null;
  photos: string[];
  createdBy: string;
  createdByName: string;
}

export interface NewIncidentReply {
  id: string;
  incidentId: string;
  senderEmail: string;
  senderName: string;
  senderRole: string;
  message: string;
  photos: string[];
}

export interface IncidentRepository {
  list(groupUid: string, filter: IncidentFilterParams): Promise<IncidentEntity[]>;
  findById(id: string, groupUid: string): Promise<IncidentEntity | null>;
  listReplies(incidentId: string): Promise<IncidentReplyEntity[]>;
  create(data: NewIncident): Promise<IncidentEntity>;
  addReply(data: NewIncidentReply): Promise<IncidentReplyEntity>;
  /** Only changes an OPEN incident. Returns the latest state either way. */
  resolve(id: string, by: { email: string; name: string }): Promise<IncidentEntity | null>;
  softDelete(id: string): Promise<Date>;
}

export type IncidentEvent =
  | { type: 'new_reply'; reply: IncidentReplyEntity }
  | {
      type: 'thread_resolved';
      incidentId: string;
      resolvedBy: string;
      resolvedByName: string;
      resolvedAt: Date | string;
    }
  | { type: 'incident_deleted'; incidentId: string; deletedAt: Date | string };

export interface IncidentEventPublisher {
  publish(incidentId: string, event: IncidentEvent): Promise<void>;
}
