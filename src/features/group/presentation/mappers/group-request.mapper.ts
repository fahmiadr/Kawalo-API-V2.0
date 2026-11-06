import type { AuthenticatedUserPayload } from '@core/types/jwt-payload.types';
import type { GetGroupMembersInput } from '@features/group/usecases/get-group-members.usecase';

export class GroupRequestMapper {
  public static toGetGroupMembersInput(
    currentUser: AuthenticatedUserPayload,
  ): GetGroupMembersInput {
    return {
      currentUser,
    };
  }
}
