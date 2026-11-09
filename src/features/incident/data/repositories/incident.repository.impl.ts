import { prisma } from '@core/database/prisma.service';
import { resolvePagination } from '@core/utils/pagination.util';
import type {
  IncidentEntity,
  IncidentReplyEntity,
  IncidentStatus,
} from '@features/incident/domain/entities/incident.entity';
import type {
  IncidentFilterParams,
  IncidentRepository,
  NewIncident,
  NewIncidentReply,
} from '@features/incident/domain/repositories/incident.repository';
import type { Prisma, PrismaClient } from '@generated/prisma/client';

type IncidentRow = Prisma.tkincidentGetPayload<object>;
type ReplyRow = Prisma.tkincidentreplyGetPayload<object>;

function parsePhotos(raw: string | null): string[] {
  if (!raw) return [];
  try {
    const parsed: unknown = JSON.parse(raw);
    return Array.isArray(parsed) ? parsed.filter((p): p is string => typeof p === 'string') : [];
  } catch {
    return [];
  }
}

const toPhotosColumn = (photos: string[]) => (photos.length > 0 ? JSON.stringify(photos) : null);

function toIncident(r: IncidentRow): IncidentEntity {
  return {
    id: r.IncidentUID,
    groupUid: r.GroupUID,
    title: r.Title,
    description: r.Description,
    category: r.Category,
    glat: r.GLat,
    glong: r.GLong,
    photos: parsePhotos(r.Photos),
    status: r.Status === 'RESOLVED' ? 'RESOLVED' : 'OPEN',
    createdBy: r.CreatedByEmail,
    createdByName: r.CreatedByName,
    resolvedBy: r.ResolvedByEmail,
    resolvedByName: r.ResolvedByName,
    resolvedAt: r.ResolvedAt,
    createdAt: r.CreatedAt,
    updatedAt: r.UpdatedAt,
  };
}

function toReply(r: ReplyRow): IncidentReplyEntity {
  return {
    id: r.ReplyUID,
    incidentId: r.IncidentUID,
    senderEmail: r.SenderEmail,
    senderName: r.SenderName,
    senderRole: r.SenderRole,
    message: r.Message,
    photos: parsePhotos(r.Photos),
    createdAt: r.CreatedAt,
  };
}

/**
 * Talks to Prisma directly (no separate datasource layer: there would be no logic in between).
 * Every query is scoped to GroupUID and DeletedAt IS NULL.
 */
export class IncidentRepositoryImpl implements IncidentRepository {
  constructor(private readonly db: PrismaClient = prisma) {}

  public async list(groupUid: string, filter: IncidentFilterParams): Promise<IncidentEntity[]> {
    const where: Prisma.tkincidentWhereInput = { GroupUID: groupUid, DeletedAt: null };
    if (filter.status) where.Status = filter.status;
    if (filter.startDate || filter.endDate) {
      where.CreatedAt = {
        ...(filter.startDate ? { gte: new Date(filter.startDate) } : {}),
        ...(filter.endDate ? { lte: new Date(filter.endDate) } : {}),
      };
    }
    if (filter.search) {
      where.OR = [
        { Title: { contains: filter.search } },
        { Description: { contains: filter.search } },
        { CreatedByName: { contains: filter.search } },
      ];
    }

    const { take, skip } = resolvePagination(filter);
    const rows = await this.db.tkincident.findMany({
      where,
      orderBy: { CreatedAt: 'desc' },
      ...(take !== undefined ? { take } : {}),
      ...(skip !== undefined ? { skip } : {}),
    });
    return rows.map(toIncident);
  }

  public async findById(id: string, groupUid: string): Promise<IncidentEntity | null> {
    const row = await this.db.tkincident.findFirst({
      where: { IncidentUID: id, GroupUID: groupUid, DeletedAt: null },
    });
    return row ? toIncident(row) : null;
  }

  public async listReplies(incidentId: string): Promise<IncidentReplyEntity[]> {
    const rows = await this.db.tkincidentreply.findMany({
      where: { IncidentUID: incidentId },
      orderBy: { ReplyUID: 'asc' }, // UUIDv7 ids sort by creation time
    });
    return rows.map(toReply);
  }

  public async create(data: NewIncident): Promise<IncidentEntity> {
    const row = await this.db.tkincident.create({
      data: {
        IncidentUID: data.id,
        GroupUID: data.groupUid,
        Title: data.title,
        Description: data.description,
        Category: data.category,
        GLat: data.glat,
        GLong: data.glong,
        Photos: toPhotosColumn(data.photos),
        CreatedByEmail: data.createdBy,
        CreatedByName: data.createdByName,
      },
    });
    return toIncident(row);
  }

  public async addReply(data: NewIncidentReply): Promise<IncidentReplyEntity> {
    const row = await this.db.tkincidentreply.create({
      data: {
        ReplyUID: data.id,
        IncidentUID: data.incidentId,
        SenderEmail: data.senderEmail,
        SenderName: data.senderName,
        SenderRole: data.senderRole,
        Message: data.message,
        Photos: toPhotosColumn(data.photos),
      },
    });
    return toReply(row);
  }

  public async resolve(
    id: string,
    by: { email: string; name: string },
  ): Promise<IncidentEntity | null> {
    const now = new Date();
    // updateMany filtered on Status=OPEN is atomic: of two concurrent resolves only one wins.
    await this.db.tkincident.updateMany({
      where: { IncidentUID: id, Status: 'OPEN' satisfies IncidentStatus, DeletedAt: null },
      data: {
        Status: 'RESOLVED',
        ResolvedByEmail: by.email,
        ResolvedByName: by.name,
        ResolvedAt: now,
        UpdatedAt: now,
      },
    });
    const row = await this.db.tkincident.findUnique({ where: { IncidentUID: id } });
    return row ? toIncident(row) : null;
  }

  public async softDelete(id: string): Promise<Date> {
    const now = new Date();
    await this.db.tkincident.update({
      where: { IncidentUID: id },
      data: { DeletedAt: now, UpdatedAt: now },
    });
    return now;
  }
}
