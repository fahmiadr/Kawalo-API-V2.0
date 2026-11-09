import type { AuthenticatedUserPayload } from '@core/types/jwt-payload.types';
import { cleanQueryDate, cleanQueryString, cleanString } from '@core/utils/clean-data.util';
import { resolvePagination } from '@core/utils/pagination.util';
import type {
  IncidentEntity,
  IncidentReplyEntity,
  IncidentStatus,
} from '@features/incident/domain/entities/incident.entity';
import type { IncidentEvent } from '@features/incident/domain/repositories/incident.repository';
import type {
  CreateIncidentDtoType,
  GetIncidentsQueryDtoType,
  PostIncidentReplyDtoType,
} from '@features/incident/presentation/dtos/incident-request.dto';
import type { CreateIncidentInput } from '@features/incident/usecases/create-incident.usecase';
import type { IncidentDetail } from '@features/incident/usecases/get-incident-detail.usecase';
import type { GetIncidentsInput } from '@features/incident/usecases/get-incidents.usecase';

/** Date atau string ISO (event dari Redis sudah berupa string setelah JSON round-trip). */
const iso = (d: Date | string): string => new Date(d).toISOString();

const cleanPhotos = (photos?: string[]): string[] =>
  (photos ?? []).map((p) => cleanString(p)).filter((p): p is string => Boolean(p));

export class IncidentMapper {
  // ---------- request ----------
  public static toGetIncidentsInput(
    query: GetIncidentsQueryDtoType,
    currentUser: AuthenticatedUserPayload,
  ): GetIncidentsInput {
    const { take, skip } = resolvePagination({ limit: query.limit, offset: query.offset });
    const status = cleanQueryString(query.status)?.toUpperCase();
    return {
      currentUser,
      status: status === 'OPEN' || status === 'RESOLVED' ? (status as IncidentStatus) : undefined,
      search: cleanQueryString(query.search),
      startDate: cleanQueryDate(query.start_date),
      endDate: cleanQueryDate(query.end_date),
      limit: take,
      offset: skip,
    };
  }

  public static toCreateInput(
    body: CreateIncidentDtoType,
    currentUser: AuthenticatedUserPayload,
  ): CreateIncidentInput {
    return {
      currentUser,
      data: {
        title: cleanQueryString(body.title) ?? '',
        description: cleanQueryString(body.description),
        category: cleanQueryString(body.category),
        glat: body.glat ?? undefined,
        glong: body.glong ?? undefined,
        photos: cleanPhotos(body.photos),
      },
    };
  }

  public static toReplyData(body: PostIncidentReplyDtoType) {
    return { message: cleanQueryString(body.message) ?? '', photos: cleanPhotos(body.photos) };
  }

  // ---------- response (snake_case sesuai kontrak app Flutter) ----------
  public static toIncidentResponse(i: IncidentEntity) {
    return {
      id: i.id,
      group_id: i.groupUid,
      title: i.title,
      description: i.description,
      category: i.category,
      glat: i.glat,
      glong: i.glong,
      photos: i.photos,
      status: i.status,
      created_by: i.createdBy,
      created_by_name: i.createdByName,
      resolved_by: i.resolvedBy,
      resolved_by_name: i.resolvedByName,
      resolved_at: i.resolvedAt ? iso(i.resolvedAt) : null,
      deleted_at: null, // insiden terhapus tidak pernah dikembalikan
      created_at: iso(i.createdAt),
      updated_at: iso(i.updatedAt),
    };
  }

  public static toReplyResponse(r: IncidentReplyEntity) {
    return {
      id: r.id,
      incident_id: r.incidentId,
      sender_id: r.senderEmail,
      sender_name: r.senderName,
      sender_role: r.senderRole,
      message: r.message,
      photos: r.photos,
      created_at: iso(r.createdAt),
    };
  }

  public static toDetailResponse({ incident, replies }: IncidentDetail) {
    return {
      incident: IncidentMapper.toIncidentResponse(incident),
      replies: replies.map(IncidentMapper.toReplyResponse),
    };
  }

  // ---------- SSE ----------
  /** Nama event & payload harus sama dengan IncidentReplyMapper.parseSseMessage di app. */
  public static toSseFrame(event: IncidentEvent): { id?: string; name: string; data: unknown } {
    switch (event.type) {
      case 'new_reply':
        return {
          id: event.reply.id,
          name: 'new_reply',
          data: IncidentMapper.toReplyResponse(event.reply),
        };
      case 'thread_resolved':
        return {
          name: 'thread_resolved',
          data: {
            incident_id: event.incidentId,
            resolved_by: event.resolvedBy,
            resolved_by_name: event.resolvedByName,
            resolved_at: iso(event.resolvedAt),
          },
        };
      case 'incident_deleted':
        return {
          name: 'incident_deleted',
          data: { incident_id: event.incidentId, deleted_at: iso(event.deletedAt) },
        };
    }
  }
}
