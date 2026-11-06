import { prisma } from '@core/database/prisma.service';
import { cleanDate, cleanInteger, cleanString } from '@core/utils/clean-data.util';
import { resolvePagination } from '@core/utils/pagination.util';
import { GuestEntity } from '@features/guest/domain/entities/guest.entity';
import type { MasterGuestTypeItem } from '@features/guest/domain/entities/master-data.entity';
import type {
  CreateGuestParams,
  GuestFilterParams,
  UpdateGuestParams,
} from '@features/guest/domain/repositories/guest.repository';
import type { Prisma, PrismaClient } from '@generated/prisma/client';

type RawGuestRecord = {
  GuestId: number;
  GuestUID: string | null;
  GuestGroupUID: string | null;
  GuestName: string | null;
  GuestNumber: string | null;
  GuestIdentity: string | null;
  GuestIdentityType: string | null;
  GuestPhoto: string | null;
  GuestDT: Date | string | null;
  GuestAckDT: Date | string | null;
  GuestOutDT: Date | string | null;
  GuestPurpose: string | null;
  GuestDestination: string;
  GuestToUserEmail: string | null;
  GuestCreatorEmail: string | null;
  GuestCreatorName: string | null;
  GuestCategory: string | null;
  QRUID: string | null;
  GuestAddress: string | null;
  GuestType: string;
  GuestCheckOutByEmail: string | null;
};

export class GuestPrismaDataSource {
  private readonly defaultGuestSelect = {
    GuestId: true,
    GuestUID: true,
    GuestGroupUID: true,
    GuestName: true,
    GuestNumber: true,
    GuestIdentity: true,
    GuestIdentityType: true,
    GuestPhoto: true,
    GuestDT: true,
    GuestAckDT: true,
    GuestOutDT: true,
    GuestPurpose: true,
    GuestDestination: true,
    GuestToUserEmail: true,
    GuestCreatorEmail: true,
    GuestCreatorName: true,
    GuestCategory: true,
    QRUID: true,
    GuestAddress: true,
    GuestType: true,
    GuestCheckOutByEmail: true,
  } as const;

  constructor(private readonly db: PrismaClient = prisma) {}

  public async getGuests(groupUid: string, filter?: GuestFilterParams): Promise<GuestEntity[]> {
    // 1. Isolasi multi-tenant grup aktif (Zero Data Poisoning)
    const whereClause: Prisma.tkguestWhereInput = {
      GuestGroupUID: groupUid,
    };

    // 2. Filter rentang tanggal kedatangan (GuestDT)
    if (filter?.startDate || filter?.endDate) {
      whereClause.GuestDT = {};
      if (filter.startDate) {
        whereClause.GuestDT.gte = new Date(filter.startDate);
      }
      if (filter.endDate) {
        whereClause.GuestDT.lte = new Date(filter.endDate);
      }
    }

    // 3. Filter pencarian multi-kolom
    if (filter?.search) {
      const search = filter.search;
      const orConditions: Prisma.tkguestWhereInput[] = [
        { GuestName: { contains: search } },
        { GuestNumber: { contains: search } },
        { GuestIdentityType: { contains: search } },
        { GuestIdentity: { contains: search } },
        { GuestCategory: { contains: search } },
        { GuestType: { contains: search } },
        { GuestAddress: { contains: search } },
        { GuestPurpose: { contains: search } },
        { GuestDestination: { contains: search } },
        { GuestToUserEmail: { contains: search } },
        { GuestCreatorEmail: { contains: search } },
        { GuestCreatorName: { contains: search } },
        { GuestCheckOutByEmail: { contains: search } },
        { QRUID: { contains: search } },
      ];

      // Prapencarian nama pengguna di tkuser yang di-scope pada anggota grup aktif
      const memberEmails = await this.findActiveMemberEmailsByGroup(groupUid);

      const matchingUsers =
        memberEmails.length > 0
          ? await this.db.tkuser.findMany({
              where: {
                UserEmail: { in: memberEmails },
                UserName: { contains: search },
              },
              select: { UserEmail: true },
            })
          : [];

      const matchingUserEmails = Array.from(
        new Set(
          matchingUsers
            .map((u) => cleanString(u.UserEmail))
            .filter((email): email is string => Boolean(email)),
        ),
      );

      if (matchingUserEmails.length > 0) {
        orConditions.push(
          { GuestToUserEmail: { in: matchingUserEmails } },
          { GuestCheckOutByEmail: { in: matchingUserEmails } },
        );
      }

      whereClause.OR = orConditions;
    }

    // 4. Filter berdasarkan email pembuat (created_by) - Exact match
    if (filter?.createdBy) {
      whereClause.GuestCreatorEmail = filter.createdBy;
    }

    // 5. Filter status kunjungan aktif (isActive: true = belum checkout, false = sudah checkout)
    if (filter?.isActive === true) {
      whereClause.GuestOutDT = null;
    } else if (filter?.isActive === false) {
      whereClause.GuestOutDT = { not: null };
    }

    const { take, skip } = resolvePagination(filter);

    // 5. Query utama ke tkguest
    const records = await this.db.tkguest.findMany({
      where: whereClause,
      orderBy: { GuestDT: 'desc' },
      select: this.defaultGuestSelect,
      ...(take !== undefined ? { take } : {}),
      ...(skip !== undefined ? { skip } : {}),
    });

    return this.mapRecordsToEntities(records);
  }

  public async getGuestById(id: string, groupUid: string): Promise<GuestEntity | null> {
    const guestId = cleanInteger(id);
    const orConditions: Prisma.tkguestWhereInput[] = [{ QRUID: id }];
    if (guestId !== null && guestId > 0) {
      orConditions.push({ GuestId: guestId });
    }

    const record = await this.db.tkguest.findFirst({
      where: {
        GuestGroupUID: groupUid,
        OR: orConditions,
      },
      select: this.defaultGuestSelect,
    });

    if (!record) {
      return null;
    }

    const [entity] = await this.mapRecordsToEntities([record]);
    return entity ?? null;
  }

  public async getGuestsByIds(ids: string[], groupUid: string): Promise<GuestEntity[]> {
    const guestIds = ids
      .map((id) => cleanInteger(id))
      .filter((id): id is number => id !== null && id > 0);

    if (guestIds.length === 0) {
      return [];
    }

    const records = await this.db.tkguest.findMany({
      where: {
        GuestId: { in: guestIds },
        GuestGroupUID: groupUid,
      },
      select: this.defaultGuestSelect,
    });

    return this.mapRecordsToEntities(records);
  }

  public async createGuest(params: CreateGuestParams): Promise<GuestEntity> {
    const { system, context, data } = params;

    const created = await this.db.tkguest.create({
      data: {
        GuestUID: system.guestUid,
        GuestGroupUID: context.groupUid,
        GuestCreatorEmail: context.creatorEmail,
        GuestCreatorName: context.creatorName,
        GuestName: data.guestName,
        GuestDestination: data.guestDestination,
        GuestPurpose: data.guestPurpose ?? null,
        GuestNumber: data.guestNumber ?? null,
        GuestIdentity: data.guestIdentity ?? null,
        GuestIdentityType: data.guestIdentityType ?? null,
        GuestCategory: system.guestCategory,
        GuestType: data.guestType,
        GuestAddress: data.guestAddress ?? null,
        GuestToUserEmail: data.guestToUserEmail ?? null,
        GuestPhoto: data.guestPhoto ?? null,
        GuestDT: system.guestDt,
        GuestAckDT: null,
        GuestOutDT: null,
        GuestCheckOutByEmail: null,
        QRUID: system.qrUid,
      },
      select: this.defaultGuestSelect,
    });

    const [entity] = await this.mapRecordsToEntities([created]);
    if (!entity) {
      throw new Error('Gagal memetakan data tamu yang baru dibuat');
    }
    return entity;
  }

  public async updateGuest(params: UpdateGuestParams): Promise<GuestEntity> {
    const { context, data, system } = params;
    const guestId = cleanInteger(context.id);
    if (guestId === null || guestId <= 0) {
      throw new Error('ID tamu harus berupa angka yang valid');
    }

    const updateData: Prisma.tkguestUpdateInput = {};

    if (data) {
      if (data.guestName !== undefined && data.guestName !== null) {
        updateData.GuestName = data.guestName;
      }
      if (data.guestDestination !== undefined && data.guestDestination !== null) {
        updateData.GuestDestination = data.guestDestination;
      }
      if (data.guestPurpose !== undefined) {
        updateData.GuestPurpose = data.guestPurpose;
      }
      if (data.guestNumber !== undefined) {
        updateData.GuestNumber = data.guestNumber;
      }
      if (data.guestIdentity !== undefined) {
        updateData.GuestIdentity = data.guestIdentity;
      }
      if (data.guestIdentityType !== undefined) {
        updateData.GuestIdentityType = data.guestIdentityType;
      }
      if (data.guestCategory !== undefined) {
        updateData.GuestCategory = data.guestCategory;
      }
      if (data.guestType !== undefined && data.guestType !== null) {
        updateData.GuestType = data.guestType;
      }
      if (data.guestAddress !== undefined) {
        updateData.GuestAddress = data.guestAddress;
      }
      if (data.guestToUserEmail !== undefined) {
        updateData.GuestToUserEmail = data.guestToUserEmail;
      }
      if (data.guestPhoto !== undefined) {
        updateData.GuestPhoto = data.guestPhoto;
      }
    }

    if (context.checkOutByEmail !== undefined) {
      updateData.GuestCheckOutByEmail = context.checkOutByEmail;
    }

    if (system) {
      if (system.guestOutDt !== undefined) {
        updateData.GuestOutDT = system.guestOutDt;
      }
      if (system.guestAckDt !== undefined) {
        updateData.GuestAckDT = system.guestAckDt;
      }
    }

    const updated = await this.db.tkguest.update({
      where: { GuestId: guestId },
      data: updateData,
      select: this.defaultGuestSelect,
    });

    const [entity] = await this.mapRecordsToEntities([updated]);
    if (!entity) {
      throw new Error('Gagal memetakan data tamu yang diperbarui');
    }
    return entity;
  }

  public async deleteManyGuests(ids: string[], groupUid: string): Promise<number> {
    const guestIds = ids
      .map((id) => cleanInteger(id))
      .filter((id): id is number => id !== null && id > 0);

    if (guestIds.length === 0) {
      return 0;
    }

    const result = await this.db.tkguest.deleteMany({
      where: {
        GuestId: { in: guestIds },
        GuestGroupUID: groupUid,
      },
    });

    return result.count;
  }

  public async resolveCanonicalGuestType(
    guestType: string,
    groupUid: string,
  ): Promise<string | null> {
    const match = await this.db.tkmasterguesttype.findFirst({
      where: {
        GuestTypeGroupUID: groupUid,
        OR: [{ GuestTypeName: guestType }, { GuestTypeUID: guestType }],
      },
      select: { GuestTypeName: true },
    });

    return match?.GuestTypeName ? cleanString(match.GuestTypeName) : null;
  }

  public async isValidGuestType(guestType: string, groupUid: string): Promise<boolean> {
    const canonical = await this.resolveCanonicalGuestType(guestType, groupUid);
    return Boolean(canonical);
  }

  public async getMasterGuestTypes(groupUid: string): Promise<MasterGuestTypeItem[]> {
    const items = await this.db.tkmasterguesttype.findMany({
      where: { GuestTypeGroupUID: groupUid },
      orderBy: { GuestTypeName: 'asc' },
      select: {
        GuestTypeId: true,
        GuestTypeUID: true,
        GuestTypeName: true,
      },
    });

    return items
      .map((item) => {
        const uid = cleanString(item.GuestTypeUID);
        const name = cleanString(item.GuestTypeName);
        if (!uid || !name) {
          return null;
        }
        return {
          id: String(item.GuestTypeId),
          uid,
          name,
        };
      })
      .filter((item): item is MasterGuestTypeItem => item !== null);
  }

  public async findActiveMemberEmailsByGroup(groupUid: string): Promise<string[]> {
    const groupMembers = await this.db.tkusergroup.findMany({
      where: {
        GroupUID: groupUid,
        Status: 'JOINED',
      },
      select: { Email: true },
    });

    return Array.from(
      new Set(
        groupMembers
          .map((m) => cleanString(m.Email))
          .filter((email): email is string => Boolean(email)),
      ),
    );
  }

  public async findUsersByEmails(
    emails: string[],
  ): Promise<Array<{ userEmail: string; userName: string | null }>> {
    if (emails.length === 0 || !this.db.tkuser) {
      return [];
    }

    const users = await this.db.tkuser.findMany({
      where: {
        UserEmail: { in: emails },
      },
      select: {
        UserEmail: true,
        UserName: true,
      },
      orderBy: {
        UserName: 'asc',
      },
    });

    return users
      .map((u) => {
        const userEmail = cleanString(u.UserEmail);
        const userName = cleanString(u.UserName);
        return userEmail ? { userEmail, userName } : null;
      })
      .filter((u): u is { userEmail: string; userName: string | null } => u !== null);
  }

  public async isGroupMemberJoined(email: string, groupUid: string): Promise<boolean> {
    if (!email || !groupUid || !this.db.tkusergroup) {
      return false;
    }

    const member = await this.db.tkusergroup.findFirst({
      where: {
        GroupUID: groupUid,
        Email: email,
        Status: 'JOINED',
      },
      select: { UserGroupId: true },
    });

    return Boolean(member);
  }

  public async isValidHost(email: string, groupUid: string): Promise<boolean> {
    return this.isGroupMemberJoined(email, groupUid);
  }

  private async mapRecordsToEntities(records: RawGuestRecord[]): Promise<GuestEntity[]> {
    if (records.length === 0) {
      return [];
    }

    // Sanitasi dan deduplikasi UID/Email relasi
    const groupUids = Array.from(
      new Set(
        records
          .map((r) => cleanString(r.GuestGroupUID))
          .filter((uid): uid is string => Boolean(uid)),
      ),
    );
    const userEmails = Array.from(
      new Set(
        records
          .flatMap((r) => [cleanString(r.GuestToUserEmail), cleanString(r.GuestCheckOutByEmail)])
          .filter((email): email is string => Boolean(email)),
      ),
    );

    // Query data relasional secara eksplisit
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
            select: { UserEmail: true, UserName: true },
          })
        : [],
    ]);

    // Safe In-Memory Map Lookup
    const groupMap = new Map(
      groups
        .map((g) => [cleanString(g.GroupUID), cleanString(g.GroupName)] as const)
        .filter((entry): entry is [string, string | null] => Boolean(entry[0])),
    );
    const userMap = new Map(
      users
        .map((u) => [cleanString(u.UserEmail), cleanString(u.UserName)] as const)
        .filter((entry): entry is [string, string | null] => Boolean(entry[0])),
    );

    // Mapping ke GuestEntity menggunakan Pipeline 5-Langkah ACL
    return records.map((r) => {
      const groupUid = cleanString(r.GuestGroupUID);
      const toUserEmail = cleanString(r.GuestToUserEmail);
      const checkOutEmail = cleanString(r.GuestCheckOutByEmail);

      const groupName = groupUid ? (groupMap.get(groupUid) ?? null) : null;
      const toUserName = toUserEmail ? (userMap.get(toUserEmail) ?? null) : null;
      const checkOutByName = checkOutEmail ? (userMap.get(checkOutEmail) ?? null) : null;

      const guestDt = cleanDate(r.GuestDT);
      const guestAckDt = cleanDate(r.GuestAckDT);
      const guestOutDt = cleanDate(r.GuestOutDT);

      const guestUid = cleanString(r.GuestUID);
      const guestName = cleanString(r.GuestName);
      const guestNumber = cleanString(r.GuestNumber);
      const guestIdentity = cleanString(r.GuestIdentity);
      const guestIdentityType = cleanString(r.GuestIdentityType);
      const guestPhoto = cleanString(r.GuestPhoto);
      const guestPurpose = cleanString(r.GuestPurpose);
      const guestDestination = cleanString(r.GuestDestination);
      const guestCreatorEmail = cleanString(r.GuestCreatorEmail);
      const guestCreatorName = cleanString(r.GuestCreatorName);
      const guestCategory = cleanString(r.GuestCategory);
      const qrUid = cleanString(r.QRUID);
      const guestAddress = cleanString(r.GuestAddress);
      const guestType = cleanString(r.GuestType);

      return new GuestEntity({
        id: String(r.GuestId),
        guestUid,
        groupUid,
        groupName,
        guestName,
        guestNumber,
        guestIdentity,
        guestIdentityType,
        guestPhoto,
        guestDt,
        guestAckDt,
        guestOutDt,
        guestPurpose,
        guestDestination,
        guestToUserEmail: toUserEmail,
        guestToUserName: toUserName,
        guestCreatorEmail,
        guestCreatorName,
        guestCategory,
        qrUid,
        guestAddress,
        guestType,
        guestCheckOutByEmail: checkOutEmail,
        guestCheckOutByName: checkOutByName,
      });
    });
  }
}
