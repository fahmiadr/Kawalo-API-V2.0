import type { Failure } from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type { GroupMemberItem } from '@features/group/domain/entities/group-member.entity';
import type { GroupRepository } from '@features/group/domain/repositories/group.repository';

export interface GetGroupMembersInput {
  currentUser: {
    group_uid?: string | null;
  };
}

export class GetGroupMembersUseCase implements UseCase<GroupMemberItem[], GetGroupMembersInput> {
  constructor(private readonly repository: GroupRepository) {}

  public async execute(input: GetGroupMembersInput): Promise<Result<GroupMemberItem[], Failure>> {
    if (!input.currentUser.group_uid) {
      return Result.ok([]);
    }

    const items = await this.repository.getMembers(input.currentUser.group_uid);
    return Result.ok(items);
  }
}
