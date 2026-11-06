import { GroupPrismaDataSource } from '@features/group/data/datasources/group-prisma.datasource';
import type { GroupMemberItem } from '@features/group/domain/entities/group-member.entity';
import type { GroupRepository } from '@features/group/domain/repositories/group.repository';

export class GroupRepositoryImpl implements GroupRepository {
  constructor(private readonly prismaSource: GroupPrismaDataSource = new GroupPrismaDataSource()) {}

  public async getMembers(groupUid: string): Promise<GroupMemberItem[]> {
    if (!groupUid) {
      return [];
    }

    const memberEmails = await this.prismaSource.findActiveMemberEmailsByGroup(groupUid);
    if (memberEmails.length === 0) {
      return [];
    }

    const users = await this.prismaSource.findUsersByEmails(memberEmails);
    const userMap = new Map(
      users.map((u) => [u.userEmail, { name: u.userName, profilePic: u.userProfilePic }]),
    );

    return memberEmails
      .map((email) => {
        const userData = userMap.get(email);
        return {
          email,
          name: userData?.name ?? null,
          profilePic: userData?.profilePic ?? null,
        };
      })
      .sort((a, b) => {
        const nameA = a.name || a.email;
        const nameB = b.name || b.email;
        return nameA.localeCompare(nameB);
      });
  }
}
