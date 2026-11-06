import { prisma } from '@core/database/prisma.service';
import { cleanCoordinate, cleanDate, cleanInteger, cleanString } from '@core/utils/clean-data.util';
import { AttendanceEntity } from '@features/attendance/domain/entities/attendance.entity';
import type { AttendanceFilterParams } from '@features/attendance/domain/repositories/attendance.repository';
import type { Prisma, PrismaClient } from '@generated/prisma/client';

export class AttendancePrismaDataSource {
  constructor(private readonly db: PrismaClient = prisma) {}

  public async getAttendances(filter?: AttendanceFilterParams): Promise<AttendanceEntity[]> {
    const whereClause: Prisma.tkabsenWhereInput = {};

    // 1. Filter isolasi multi-tenant grup aktif (wajib ada untuk mencegah kebocoran data antar tenant)
    const groupUid = filter?.groupUid;
    if (!groupUid) {
      return [];
    }
    whereClause.AbsenGroupUID = groupUid;

    // 2. Filter rentang tanggal absensi (AbsenDT)
    if (filter?.startDate || filter?.endDate) {
      whereClause.AbsenDT = {};
      if (filter.startDate) {
        whereClause.AbsenDT.gte = new Date(filter.startDate);
      }
      if (filter.endDate) {
        whereClause.AbsenDT.lte = new Date(filter.endDate);
      }
    }

    // 3. Filter pencarian multi-kolom
    if (filter?.search) {
      const search = filter.search;
      // Isolasi pencarian shift hanya pada grup tenant yang aktif (mencegah cross-tenant shift poisoning)
      const matchingShifts = await this.db.tkshift.findMany({
        where: {
          ShiftGroupUID: groupUid,
          ShiftName: { contains: search },
        },
        select: { ShiftUID: true },
      });
      const matchingShiftUids = Array.from(
        new Set(
          matchingShifts
            .map((s) => cleanString(s.ShiftUID))
            .filter((uid): uid is string => Boolean(uid)),
        ),
      );

      whereClause.OR = [
        { AbsenName: { contains: search } },
        { AbsenEmail: { contains: search } },
        { AbsenType: { contains: search } },
        { ALocation: { contains: search } },
        ...(matchingShiftUids.length > 0 ? [{ AShiftUID: { in: matchingShiftUids } }] : []),
      ];
    }

    // 4. Explicit select data tkabsen yang murni dibutuhkan oleh AttendanceEntity
    const records = await this.db.tkabsen.findMany({
      where: whereClause,
      orderBy: { AbsenDT: 'desc' },
      take: filter?.limit,
      skip: filter?.offset,
      select: {
        AbsenID: true,
        AbsenEmail: true,
        AbsenName: true,
        AbsenDT: true,
        AbsenType: true,
        ALat: true,
        ALong: true,
        ADiffMins: true,
        ALocation: true,
        AShiftUID: true,
        AbsenGroupUID: true,
        AShStart: true,
      },
    });

    // 5. Sanitasi dan deduplikasi UID/Email untuk mencegah query bloat dan data poisoning
    const groupUids = Array.from(
      new Set(
        records
          .map((r) => cleanString(r.AbsenGroupUID))
          .filter((uid): uid is string => Boolean(uid)),
      ),
    );

    const userEmails = Array.from(
      new Set(
        records
          .map((r) => cleanString(r.AbsenEmail))
          .filter((email): email is string => Boolean(email)),
      ),
    );

    const shiftUids = Array.from(
      new Set(
        records.map((r) => cleanString(r.AShiftUID)).filter((uid): uid is string => Boolean(uid)),
      ),
    );

    // 6. Explicit select data relasional secara terisolasi
    const [groups, users, shifts] = await Promise.all([
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
              UserDuty: true,
            },
          })
        : [],
      shiftUids.length > 0
        ? this.db.tkshift.findMany({
            where: {
              ShiftUID: { in: shiftUids },
              ShiftGroupUID: groupUid,
            },
            select: {
              ShiftUID: true,
              ShiftName: true,
              ShiftColor: true,
              ShiftTimeStart: true,
              ShiftTimeStart_mm: true,
              ShiftTimeStop: true,
              ShiftTimeStop_mm: true,
            },
          })
        : [],
    ]);

    // 7. Safe In-Memory Map Lookup (Email otomatis lowercase via cleanString)
    const groupMap = new Map(
      groups
        .map((g) => [cleanString(g.GroupUID), cleanString(g.GroupName)] as const)
        .filter((entry): entry is [string, string | null] => Boolean(entry[0])),
    );
    const userMap = new Map(
      users
        .map((u) => [cleanString(u.UserEmail), u] as const)
        .filter((entry): entry is [string, (typeof users)[number]] => Boolean(entry[0])),
    );
    const shiftMap = new Map(
      shifts
        .map((s) => [cleanString(s.ShiftUID), s] as const)
        .filter((entry): entry is [string, (typeof shifts)[number]] => Boolean(entry[0])),
    );

    // 8. Mapping ke AttendanceEntity menggunakan Pipeline 5-Langkah ACL (100% Pra-Sanitasi & Anti-Hallucination)
    return records
      .map((r) => {
        // 1. Guard integritas: Mencegah sintesis entitas dengan id korup/hilang
        const cleanId = cleanInteger(r.AbsenID);
        if (cleanId === null || cleanId <= 0) {
          return null;
        }

        // 2. Sanitasi foreign keys relasional murni dari record DB
        const groupUid = cleanString(r.AbsenGroupUID);
        const userEmail = cleanString(r.AbsenEmail);
        const shiftUid = cleanString(r.AShiftUID);

        // 3. Safe Map Lookup data relasional
        const groupName = groupUid ? (groupMap.get(groupUid) ?? null) : null;
        const user = userEmail ? (userMap.get(userEmail) ?? null) : null;
        const shift = shiftUid ? (shiftMap.get(shiftUid) ?? null) : null;

        // 4. Sanitasi field profil pengguna & metadata shift
        const userName = cleanString(user?.UserName) ?? cleanString(r.AbsenName);
        const userProfilePic = cleanString(user?.UserProfilePic);
        const userDuty = cleanString(user?.UserDuty);
        const shiftName = cleanString(shift?.ShiftName);
        const shiftColor = cleanString(shift?.ShiftColor);

        // 5. Format & sanitasi rentang waktu shift
        let shiftTimeWindow: string | null = null;
        const shiftStartH = cleanInteger(shift?.ShiftTimeStart);
        const shiftStopH = cleanInteger(shift?.ShiftTimeStop);
        const shiftStartM = cleanInteger(shift?.ShiftTimeStart_mm) ?? 0;
        const shiftStopM = cleanInteger(shift?.ShiftTimeStop_mm) ?? 0;

        if (
          shiftStartH !== null &&
          shiftStopH !== null &&
          shiftStartH >= 0 &&
          shiftStartH <= 23 &&
          shiftStopH >= 0 &&
          shiftStopH <= 23
        ) {
          const startH = String(shiftStartH).padStart(2, '0');
          const startM = String(shiftStartM).padStart(2, '0');
          const stopH = String(shiftStopH).padStart(2, '0');
          const stopM = String(shiftStopM).padStart(2, '0');
          shiftTimeWindow = `${startH}:${startM} - ${stopH}:${stopM}`;
        } else {
          const fallbackShStart = cleanString(r.AShStart);
          if (fallbackShStart && fallbackShStart !== '00:00:00') {
            shiftTimeWindow = fallbackShStart;
          }
        }

        // 6. Sanitasi seluruh atribut absensi dari record mentah database
        const attendanceDt = cleanDate(r.AbsenDT);
        const attendanceType = cleanString(r.AbsenType);
        const latitude = cleanCoordinate(r.ALat);
        const longitude = cleanCoordinate(r.ALong);
        const locationName = cleanString(r.ALocation);
        const diffMinutes = cleanInteger(r.ADiffMins) ?? 0;

        // 7. Konstruksi AttendanceEntity murni menggunakan data yang 100% sudah bersih
        return new AttendanceEntity({
          id: String(cleanId),
          userEmail,
          userName,
          userProfilePic,
          userDuty,
          attendanceDt,
          attendanceType,
          latitude,
          longitude,
          diffMinutes,
          locationName,
          shiftName,
          shiftColor,
          shiftTimeWindow,
          groupName,
        });
      })
      .filter((item): item is AttendanceEntity => item !== null);
  }
}
