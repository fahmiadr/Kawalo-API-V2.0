export const INCIDENT_CATEGORIES = ['facility', 'security', 'hazard', 'general'] as const;
export type IncidentCategory = (typeof INCIDENT_CATEGORIES)[number];
export type IncidentStatus = 'OPEN' | 'RESOLVED';

export interface IncidentEntity {
  id: string;
  groupUid: string;
  title: string;
  description: string | null;
  category: string;
  glat: number | null;
  glong: number | null;
  photos: string[];
  status: IncidentStatus;
  createdBy: string;
  createdByName: string | null;
  resolvedBy: string | null;
  resolvedByName: string | null;
  resolvedAt: Date | null;
  createdAt: Date;
  updatedAt: Date;
}

export interface IncidentReplyEntity {
  id: string;
  incidentId: string;
  senderEmail: string;
  senderName: string;
  senderRole: string;
  message: string;
  photos: string[];
  createdAt: Date;
}
