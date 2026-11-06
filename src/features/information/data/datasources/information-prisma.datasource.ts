import { prisma } from '@core/database/prisma.service';
import { cleanCoordinate, cleanDate, cleanInteger, cleanString } from '@core/utils/clean-data.util';
import { resolvePagination } from '@core/utils/pagination.util';
import { InformationEntity } from '@features/information/domain/entities/information.entity';
import type {
  CreateInformationParams,
  FeedCreatorContext,
  InformationFilterParams,
  UpdateInformationParams,
} from '@features/information/domain/repositories/information.repository';
import type { Prisma, PrismaClient } from '@generated/prisma/client';

type RawFeedRecord = {
  FeedId: number;
  FeedUID: string | null;
  FeedTitle: string;
  FeedText: string | null;
  FeedByEmail: string;
  FeedByName: string | null;
  FeedDT: Date | string | null;
  FeedFile: string | null;
  FeedPic: string | null;
  FeedPic2: string | null;
  FeedPic3: string | null;
  FeedPic4: string | null;
  FeedType: string | null;
  FeedByProfilePic: string | null;
  FeedGroupUID: string;
  FeedGroupName: string | null;
  FeedEventLocation: string | null;
  FeedEventStartDT: Date | string | null;
  FeedEventEndDT: Date | string | null;
  FeedEventDuration: number | null;
  GLat: string | null;
  GLong: string | null;
};

export class InformationPrismaDataSource {
  private readonly defaultFeedSelect = {
    FeedId: true,
    FeedUID: true,
    FeedTitle: true,
    FeedText: true,
    FeedByEmail: true,
    FeedByName: true,
    FeedDT: true,
    FeedFile: true,
    FeedPic: true,
    FeedPic2: true,
    FeedPic3: true,
    FeedPic4: true,
    FeedType: true,
    FeedByProfilePic: true,
    FeedGroupUID: true,
    FeedGroupName: true,
    FeedEventLocation: true,
    FeedEventStartDT: true,
    FeedEventEndDT: true,
    FeedEventDuration: true,
    GLat: true,
    GLong: true,
  } as const;

  constructor(private readonly db: PrismaClient = prisma) {}

  public async getInformation(
    groupUid: string,
    filter?: InformationFilterParams,
  ): Promise<InformationEntity[]> {
    // 1. Isolasi multi-tenant grup aktif (Zero Data Poisoning)
    const whereClause: Prisma.tkfeedWhereInput = {
      FeedGroupUID: groupUid,
    };

    // 2. Filter rentang tanggal publikasi (FeedDT) - Menggunakan input yang telah disanitasi di mapper
    if (filter?.startDate || filter?.endDate) {
      whereClause.FeedDT = {};
      if (filter.startDate) {
        whereClause.FeedDT.gte = new Date(filter.startDate);
      }
      if (filter.endDate) {
        whereClause.FeedDT.lte = new Date(filter.endDate);
      }
    }

    // 3. Filter kata kunci pencarian - Menggunakan input yang telah disanitasi di mapper
    if (filter?.search) {
      whereClause.OR = [
        { FeedTitle: { contains: filter.search } },
        { FeedText: { contains: filter.search } },
        { FeedByName: { contains: filter.search } },
        { FeedByEmail: { contains: filter.search } },
        { FeedType: { contains: filter.search } },
        { FeedEventLocation: { contains: filter.search } },
      ];
    }

    // 4. Filter berdasarkan email pembuat (created_by) - Exact match
    if (filter?.createdBy) {
      whereClause.FeedByEmail = filter.createdBy;
    }

    // 5. Penanganan paginasi
    const { take, skip } = resolvePagination(filter);

    // 5. Explicit select data
    const records = await this.db.tkfeed.findMany({
      where: whereClause,
      orderBy: { FeedDT: 'desc' },
      select: this.defaultFeedSelect,
      ...(take !== undefined ? { take } : {}),
      ...(skip !== undefined ? { skip } : {}),
    });

    return this.mapRecordsToEntities(records);
  }

  public async getInformationById(id: string, groupUid: string): Promise<InformationEntity | null> {
    const feedId = cleanInteger(id);
    if (feedId === null || feedId <= 0) {
      return null;
    }

    const record = await this.db.tkfeed.findFirst({
      where: {
        FeedId: feedId,
        FeedGroupUID: groupUid,
      },
      select: this.defaultFeedSelect,
    });

    if (!record) {
      return null;
    }

    const [entity] = await this.mapRecordsToEntities([record]);
    return entity ?? null;
  }

  public async getInformationByIds(ids: string[], groupUid: string): Promise<InformationEntity[]> {
    const feedIds = ids
      .map((id) => cleanInteger(id))
      .filter((id): id is number => id !== null && id > 0);

    if (feedIds.length === 0) {
      return [];
    }

    const records = await this.db.tkfeed.findMany({
      where: {
        FeedId: { in: feedIds },
        FeedGroupUID: groupUid,
      },
      select: this.defaultFeedSelect,
    });

    return this.mapRecordsToEntities(records);
  }

  public async getFeedCreatorContext(
    groupUid: string,
    userEmail: string,
  ): Promise<FeedCreatorContext> {
    const [group, user] = await Promise.all([
      this.db.tkgroup.findUnique({
        where: { GroupUID: groupUid },
        select: { GroupName: true },
      }),
      this.db.tkuser.findUnique({
        where: { UserEmail: userEmail },
        select: { UserProfilePic: true },
      }),
    ]);

    return {
      groupName: cleanString(group?.GroupName) ?? null,
      userProfilePic: cleanString(user?.UserProfilePic) ?? null,
    };
  }

  public async createInformation(params: CreateInformationParams): Promise<InformationEntity> {
    const { system, context, data } = params;

    const created = await this.db.tkfeed.create({
      data: {
        FeedUID: system.feedUid,
        FeedGroupUID: context.groupUid,
        FeedByEmail: context.creatorEmail,
        FeedByName: context.creatorName,
        FeedGroupName: system.feedGroupName ?? null,
        FeedByProfilePic: system.feedByProfilePic ?? null,
        FeedDT: system.feedDt,
        FeedTitle: data.title,
        FeedText: data.text,
        FeedType: data.feedType,
        FeedFile: data.feedFile,
        FeedPic: data.feedPic,
        FeedPic2: data.feedPic2,
        FeedPic3: data.feedPic3,
        FeedPic4: data.feedPic4,
        FeedEventLocation: data.eventLocation,
        GLat: data.glat,
        GLong: data.glong,
      },
      select: this.defaultFeedSelect,
    });

    const [entity] = await this.mapRecordsToEntities([created]);
    if (!entity) {
      throw new Error('Gagal memetakan data informasi yang baru dibuat');
    }
    return entity;
  }

  public async updateInformation(params: UpdateInformationParams): Promise<InformationEntity> {
    const { context, data } = params;
    const feedId = cleanInteger(context.id);
    if (feedId === null || feedId <= 0) {
      throw new Error('ID informasi harus berupa bilangan bulat positif yang valid');
    }

    const updateData: Prisma.tkfeedUpdateInput = {};

    if (data.title !== undefined && data.title !== null) {
      updateData.FeedTitle = data.title;
    }
    if (data.text !== undefined) updateData.FeedText = data.text;
    if (data.feedType !== undefined) updateData.FeedType = data.feedType;
    if (data.feedFile !== undefined) updateData.FeedFile = data.feedFile;
    if (data.feedPic !== undefined) updateData.FeedPic = data.feedPic;
    if (data.feedPic2 !== undefined) updateData.FeedPic2 = data.feedPic2;
    if (data.feedPic3 !== undefined) updateData.FeedPic3 = data.feedPic3;
    if (data.feedPic4 !== undefined) updateData.FeedPic4 = data.feedPic4;
    if (data.eventLocation !== undefined) updateData.FeedEventLocation = data.eventLocation;
    if (data.glat !== undefined) updateData.GLat = data.glat;
    if (data.glong !== undefined) updateData.GLong = data.glong;

    const updated = await this.db.tkfeed.update({
      where: { FeedId: feedId, FeedGroupUID: context.groupUid },
      data: updateData,
      select: this.defaultFeedSelect,
    });

    const [entity] = await this.mapRecordsToEntities([updated]);
    if (!entity) {
      throw new Error('Gagal memetakan data informasi yang diperbarui');
    }
    return entity;
  }

  public async deleteManyInformation(ids: string[], groupUid: string): Promise<number> {
    const feedIds = ids
      .map((id) => cleanInteger(id))
      .filter((id): id is number => id !== null && id > 0);

    if (feedIds.length === 0) {
      return 0;
    }

    const result = await this.db.tkfeed.deleteMany({
      where: {
        FeedId: { in: feedIds },
        FeedGroupUID: groupUid,
      },
    });

    return result.count;
  }

  private async mapRecordsToEntities(records: RawFeedRecord[]): Promise<InformationEntity[]> {
    if (records.length === 0) {
      return [];
    }

    // Sanitasi dan deduplikasi UID/Email untuk mencegah query bloat dan data poisoning
    const groupUids = Array.from(
      new Set(
        records
          .map((r) => cleanString(r.FeedGroupUID))
          .filter((uid): uid is string => Boolean(uid)),
      ),
    );

    const userEmails = Array.from(
      new Set(
        records
          .map((r) => cleanString(r.FeedByEmail))
          .filter((email): email is string => Boolean(email)),
      ),
    );

    // Explicit select data relasional secara terisolasi
    const [groups, users] = await Promise.all([
      groupUids.length > 0
        ? this.db.tkgroup.findMany({
            where: { GroupUID: { in: groupUids } },
            select: { GroupUID: true, GroupName: true },
          })
        : [],
      userEmails.length > 0
        ? this.db.tkuser.findMany({
            where: { UserEmail: { in: userEmails } },
            select: {
              UserEmail: true,
              UserName: true,
              UserProfilePic: true,
            },
          })
        : [],
    ]);

    // Safe In-Memory Map Lookup (Data dari DB disanitasi sebelum dimasukkan ke map dan digunakan)
    const groupMap = new Map(
      groups
        .map((g) => [cleanString(g.GroupUID), cleanString(g.GroupName)] as const)
        .filter((entry): entry is [string, string | null] => Boolean(entry[0])),
    );

    const userMap = new Map(
      users
        .map((u) => {
          const email = cleanString(u.UserEmail);
          const name = cleanString(u.UserName);
          const profilePic = cleanString(u.UserProfilePic);
          return email ? ([email, { userName: name, userProfilePic: profilePic }] as const) : null;
        })
        .filter(
          (
            entry,
          ): entry is readonly [
            string,
            { userName: string | null; userProfilePic: string | null },
          ] => entry !== null,
        ),
    );

    // Mapping ke InformationEntity menggunakan Pipeline ACL (100% data tersanitasi)
    return records.map((r) => {
      const recordGroupUid = cleanString(r.FeedGroupUID);
      const userEmail = cleanString(r.FeedByEmail);

      const groupNameFromMap = recordGroupUid ? (groupMap.get(recordGroupUid) ?? null) : null;
      const userFromMap = userEmail ? (userMap.get(userEmail) ?? null) : null;

      const userName = cleanString(r.FeedByName) ?? userFromMap?.userName ?? null;
      const userProfilePic = cleanString(r.FeedByProfilePic) ?? userFromMap?.userProfilePic ?? null;
      const groupName = cleanString(r.FeedGroupName) ?? groupNameFromMap;

      const photos: string[] = [];
      const p1 = cleanString(r.FeedPic);
      const p2 = cleanString(r.FeedPic2);
      const p3 = cleanString(r.FeedPic3);
      const p4 = cleanString(r.FeedPic4);
      if (p1) photos.push(p1);
      if (p2) photos.push(p2);
      if (p3) photos.push(p3);
      if (p4) photos.push(p4);

      const cleanFeedId = cleanInteger(r.FeedId);
      const id = String(cleanFeedId);
      const title = cleanString(r.FeedTitle);
      const text = cleanString(r.FeedText);
      const feedDt = cleanDate(r.FeedDT);
      const feedFile = cleanString(r.FeedFile);
      const feedType = cleanString(r.FeedType);
      const eventLocation = cleanString(r.FeedEventLocation);
      const eventStartDt = cleanDate(r.FeedEventStartDT);
      const eventEndDt = cleanDate(r.FeedEventEndDT);
      const eventDuration = cleanInteger(r.FeedEventDuration);
      const glat = cleanCoordinate(r.GLat);
      const glong = cleanCoordinate(r.GLong);

      return new InformationEntity({
        id,
        title,
        text,
        userEmail,
        userName,
        userProfilePic,
        feedDt,
        feedFile,
        photos,
        feedType,
        groupName,
        eventLocation,
        eventStartDt,
        eventEndDt,
        eventDuration,
        glat,
        glong,
      });
    });
  }
}
