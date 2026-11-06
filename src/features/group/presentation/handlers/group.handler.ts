import type { AuthenticatedHandlerContext } from '@core/types/context.types';
import { ApiResponse } from '@core/utils/api-response';
import { GroupRequestMapper } from '@features/group/presentation/mappers/group-request.mapper';
import { GroupResponseMapper } from '@features/group/presentation/mappers/group-response.mapper';
import type { GetGroupMembersUseCase } from '@features/group/usecases/get-group-members.usecase';

export interface GetGroupMembersContext extends AuthenticatedHandlerContext {}

export class GroupHandler {
  constructor(private readonly getGroupMembersUseCase: GetGroupMembersUseCase) {}

  public async getMembers({ currentUser, set }: GetGroupMembersContext) {
    const input = GroupRequestMapper.toGetGroupMembersInput(currentUser);
    const result = await this.getGroupMembersUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    const responseData = GroupResponseMapper.toGroupMemberList(result.value);
    return ApiResponse.success(responseData, 'Daftar anggota grup berhasil diambil');
  }
}
