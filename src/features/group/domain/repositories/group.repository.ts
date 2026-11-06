import type { GroupMemberItem } from '@features/group/domain/entities/group-member.entity';

export interface GroupRepository {
  getMembers(groupUid: string): Promise<GroupMemberItem[]>;
}
