import { GroupRepositoryImpl } from '@features/group/data/repositories/group.repository.impl';
import { GroupHandler } from '@features/group/presentation/handlers/group.handler';
import { GetGroupMembersUseCase } from '@features/group/usecases/get-group-members.usecase';

const groupRepository = new GroupRepositoryImpl();
const getGroupMembersUseCase = new GetGroupMembersUseCase(groupRepository);

export const groupHandler = new GroupHandler(getGroupMembersUseCase);
