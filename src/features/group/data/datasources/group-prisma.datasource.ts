import { prisma } from '@core/database/prisma.service';
import { cleanString } from '@core/utils/clean-data.util';
import type { PrismaClient } from '@generated/prisma/client';

export class GroupPrismaDataSource {
  constructor(private readonly db: PrismaClient = prisma) {}

  public async findActiveMemberEmailsByGroup(groupUid: string): Promise<string[]> {
    if (!groupUid || !this.db.tkusergroup) {
      return [];
    }

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
  ): Promise<Array<{ userEmail: string; userName: string | null; userProfilePic: string | null }>> {
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
        UserProfilePic: true,
      },
      orderBy: {
        UserName: 'asc',
      },
    });

    return users
      .map((u) => {
        const userEmail = cleanString(u.UserEmail);
        const userName = cleanString(u.UserName);
        const userProfilePic = cleanString(u.UserProfilePic);
        return userEmail ? { userEmail, userName, userProfilePic } : null;
      })
      .filter(
        (u): u is { userEmail: string; userName: string | null; userProfilePic: string | null } =>
          u !== null,
      );
  }
}
