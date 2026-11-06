import { prisma } from '@core/database/prisma.service';
import { cleanDate, cleanInteger, cleanString, toLocalDatetime } from '@core/utils/clean-data.util';
import {
  DutyTimelineEntity,
  OperationalEntity,
} from '@features/operational/domain/entities/operational.entity';
import type { OperationalFilterParams } from '@features/operational/domain/repositories/operational.repository';
import { Prisma, type PrismaClient } from '@generated/prisma/client';

interface DistinctOperationalDayRow {
  userEmail: string;
  dutyDate: string;
  maxDT: Date;
}

interface DutyRecordRow {
  DutyJourneyId: number;
  DutyJourneyUID: string | null;
  DutyGroupUID: string | null;
  DutyGroupName: string | null;
  DutyDuty: string | null;
  DutyState: string | null;
  DutyDutyDT: Date | string | null;
  DutyStateDT: Date | string | null;
  UserEmail: string | null;
  UserName: string | null;
  dutyDate: string | null;
}

export class OperationalPrismaDataSource {
  constructor(private readonly db: PrismaClient = prisma) {}

  public async getOperationals(filter?: OperationalFilterParams): Promise<OperationalEntity[]> {
    // 1. Filter isolasi multi-tenant grup aktif (wajib ada untuk mencegah kebocoran data antar tenant)
    const groupUid = filter?.groupUid;
    if (!groupUid) {
      return [];
    }

    const conditions: Prisma.Sql[] = [];
    conditions.push(Prisma.sql`DutyGroupUID = ${groupUid}`);

    // 2. Filter rentang tanggal berdasarkan kolom DutyDutyDT
    if (filter?.startDate) {
      conditions.push(Prisma.sql`DutyDutyDT >= ${toLocalDatetime(filter.startDate)}`);
    }
    if (filter?.endDate) {
      conditions.push(Prisma.sql`DutyDutyDT <= ${toLocalDatetime(filter.endDate)}`);
    }

    // 3. Filter pencarian multi-kolom
    if (filter?.search) {
      const searchPattern = `%${filter.search}%`;
      conditions.push(
        Prisma.sql`(DutyDuty LIKE ${searchPattern} OR DutyState LIKE ${searchPattern} OR UserEmail LIKE ${searchPattern} OR UserName LIKE ${searchPattern})`,
      );
    }

    // 4. Hanya ambil personil dengan UserEmail yang valid (tidak kosong)
    conditions.push(Prisma.sql`UserEmail IS NOT NULL AND UserEmail != ''`);

    const whereSql =
      conditions.length > 0 ? Prisma.sql`WHERE ${Prisma.join(conditions, ' AND ')}` : Prisma.empty;

    const limit = filter?.limit;
    const offset = filter?.offset;
    const limitClause =
      limit === undefined
        ? Prisma.empty
        : Prisma.sql`LIMIT ${Prisma.raw(String(limit))} OFFSET ${Prisma.raw(String(offset))}`;

    // Step 1: Paginasi tingkat kombinasi personil dan hari (UserEmail + DATE(DutyDutyDT))
    // Menggunakan SELECT data eksplisit
    const rawDistinctDays = await this.db.$queryRaw<DistinctOperationalDayRow[]>(Prisma.sql`
      SELECT 
        UserEmail AS userEmail,
        DATE_FORMAT(DutyDutyDT, '%Y-%m-%d') AS dutyDate,
        MAX(DutyDutyDT) AS maxDT
      FROM tkdutyjourney
      ${whereSql}
      GROUP BY UserEmail, DATE_FORMAT(DutyDutyDT, '%Y-%m-%d')
      ORDER BY maxDT DESC
      ${limitClause}
    `);

    if (!rawDistinctDays || rawDistinctDays.length === 0) {
      return [];
    }

    // Sanitasi data hasil agregasi hari dan email (deduplikasi case-insensitive)
    const seenDayKeys = new Set<string>();
    const distinctDays: Array<{ userEmail: string; dutyDate: string }> = [];

    for (const d of rawDistinctDays) {
      const email = cleanString(d.userEmail);
      const date = cleanString(d.dutyDate);
      if (!email || !date) continue;

      const dedupeKey = `${email}_${date}`;
      if (!seenDayKeys.has(dedupeKey)) {
        seenDayKeys.add(dedupeKey);
        distinctDays.push({ userEmail: email, dutyDate: date });
      }
    }

    if (distinctDays.length === 0) {
      return [];
    }

    // Step 2: Ambil seluruh log aktivitas secara efisien menggunakan index sargable (tanpa DATE_FORMAT di OR clause)
    const distinctEmails = Array.from(new Set(distinctDays.map((d) => d.userEmail)));
    const sortedDates = distinctDays.map((d) => d.dutyDate).sort();
    const minDateStr = `${sortedDates[0]} 00:00:00`;
    const maxDateStr = `${sortedDates[sortedDates.length - 1]} 23:59:59`;

    const searchCondition = filter?.search
      ? Prisma.sql`AND (DutyDuty LIKE ${`%${filter.search}%`} OR DutyState LIKE ${`%${filter.search}%`} OR UserEmail LIKE ${`%${filter.search}%`} OR UserName LIKE ${`%${filter.search}%`})`
      : Prisma.empty;

    // Explicit select data tkdutyjourney yang hanya dibutuhkan Entity & DTO
    const records = await this.db.$queryRaw<DutyRecordRow[]>(Prisma.sql`
      SELECT 
        DutyJourneyId,
        DutyJourneyUID,
        DutyGroupUID,
        DutyGroupName,
        DutyDuty,
        DutyState,
        DutyDutyDT,
        DutyStateDT,
        UserEmail,
        UserName,
        DATE_FORMAT(DutyDutyDT, '%Y-%m-%d') AS dutyDate
      FROM tkdutyjourney
      WHERE DutyGroupUID = ${groupUid}
        AND UserEmail IN (${Prisma.join(distinctEmails)})
        AND DutyDutyDT >= ${toLocalDatetime(minDateStr)}
        AND DutyDutyDT <= ${toLocalDatetime(maxDateStr)}
        ${searchCondition}
      ORDER BY DutyDutyDT DESC
    `);

    // Step 3: Bangun Safe In-Memory Grouping Map per kartu harian personil (case-insensitive email key)
    const cardMap = new Map<string, DutyRecordRow[]>();
    for (const d of distinctDays) {
      const key = `${d.userEmail}_${d.dutyDate}`;
      cardMap.set(key, []);
    }

    for (const record of records) {
      const recordEmail = cleanString(record.UserEmail);
      const recordDate = cleanString(record.dutyDate);
      if (recordEmail && recordDate) {
        const key = `${recordEmail}_${recordDate}`;
        const list = cardMap.get(key);
        if (list) {
          list.push(record);
        }
      }
    }

    // Step 4: Sanitasi & query relasi tkuser (Safe Map Lookup)
    const userPics =
      distinctEmails.length > 0
        ? await this.db.tkuser.findMany({
            where: {
              UserEmail: {
                in: distinctEmails,
              },
            },
            select: {
              UserEmail: true,
              UserProfilePic: true,
            },
          })
        : [];

    const picMap = new Map(
      userPics
        .map((u) => [cleanString(u.UserEmail), cleanString(u.UserProfilePic)] as const)
        .filter((entry): entry is [string, string | null] => Boolean(entry[0])),
    );

    // Step 5: Konstruksi Entity Murni Menggunakan Standar Pipeline ACL (100% Pra-Sanitasi)
    const result: OperationalEntity[] = [];

    for (const d of distinctDays) {
      const userEmail = d.userEmail;
      const dutyDate = d.dutyDate;
      const key = `${userEmail}_${dutyDate}`;
      const journeys = cardMap.get(key) ?? [];
      const latest = journeys[0];
      if (!latest) continue;

      // 1. Transformasi timeline menggunakan pipeline pra-sanitasi penuh (Zero Data Hallucination)
      const timeline: DutyTimelineEntity[] = journeys
        .map((j) => {
          const cleanJourneyId = cleanInteger(j.DutyJourneyId);
          const journeyUid = cleanString(j.DutyJourneyUID);
          const id =
            cleanJourneyId !== null && cleanJourneyId > 0 ? String(cleanJourneyId) : journeyUid;

          if (!id) {
            return null;
          }

          const duty = cleanString(j.DutyDuty);
          const state = cleanString(j.DutyState);
          const dutyDt = cleanDate(j.DutyDutyDT);
          const stateDt = cleanDate(j.DutyStateDT);

          return new DutyTimelineEntity({
            id,
            journeyUid,
            duty,
            state,
            dutyDt,
            stateDt,
          });
        })
        .filter((t): t is DutyTimelineEntity => t !== null);

      // 2. Pra-sanitasi seluruh atribut kartu tenaga kerja dari record mentah terbaru
      const userName =
        cleanString(latest.UserName) ??
        cleanString(journeys.find((j) => Boolean(cleanString(j.UserName)))?.UserName);
      const userProfilePic = picMap.get(userEmail) ?? null;
      const groupName =
        cleanString(latest.DutyGroupName) ??
        cleanString(journeys.find((j) => Boolean(cleanString(j.DutyGroupName)))?.DutyGroupName);
      const latestDuty = cleanString(latest.DutyDuty);
      const latestState = cleanString(latest.DutyState);
      const latestDutyDt = cleanDate(latest.DutyDutyDT);
      const latestStateDt = cleanDate(latest.DutyStateDT);
      const totalActivities = journeys.length;

      // 3. Konstruksi OperationalEntity murni (Zero Hallucination, Nullable Asli)
      result.push(
        new OperationalEntity({
          id: `${userEmail}_${dutyDate}`,
          userEmail,
          userName,
          userProfilePic,
          groupName,
          latestDuty,
          latestState,
          latestDutyDt,
          latestStateDt,
          totalActivities,
          dutyDate,
          timeline,
        }),
      );
    }

    return result;
  }
}
