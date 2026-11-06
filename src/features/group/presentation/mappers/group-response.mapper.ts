import type { GroupMemberItem } from '@features/group/domain/entities/group-member.entity';
import type { GroupMemberItemDtoType } from '@features/group/presentation/dtos/group-response.dto';

export class GroupResponseMapper {
  public static toGroupMember(item: GroupMemberItem): GroupMemberItemDtoType {
    return {
      email: item.email,
      name: item.name,
      profilePic: item.profilePic,
    };
  }

  public static toGroupMemberList(items: GroupMemberItem[]): GroupMemberItemDtoType[] {
    return items.map((item) => GroupResponseMapper.toGroupMember(item));
  }
}
