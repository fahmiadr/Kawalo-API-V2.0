import type { GroupMemberItem } from '@features/group/domain/entities/group-member.entity';
import type { GroupRepository } from '@features/group/domain/repositories/group.repository';
import { GetGroupMembersUseCase } from '@features/group/usecases/get-group-members.usecase';
import { beforeEach, describe, expect, it, vi } from 'vitest';

describe('GetGroupMembersUseCase Unit Tests', () => {
  let repository: GroupRepository;
  let useCase: GetGroupMembersUseCase;

  beforeEach(() => {
    repository = {
      getMembers: vi.fn(),
    };
    useCase = new GetGroupMembersUseCase(repository);
  });

  it('mengembalikan array kosong jika user tidak memiliki group_uid', async () => {
    const result = await useCase.execute({
      currentUser: { group_uid: null },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value).toEqual([]);
    expect(repository.getMembers).not.toHaveBeenCalled();
  });

  it('mengembalikan daftar anggota grup jika group_uid tersedia', async () => {
    const mockMembers: GroupMemberItem[] = [
      { email: 'member1@veranda.com', name: 'Member Satu', profilePic: 'https://pic1.jpg' },
      { email: 'member2@veranda.com', name: 'Member Dua', profilePic: null },
    ];
    vi.mocked(repository.getMembers).mockResolvedValue(mockMembers);

    const result = await useCase.execute({
      currentUser: { group_uid: 'GRP-001' },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value).toEqual(mockMembers);
    expect(repository.getMembers).toHaveBeenCalledWith('GRP-001');
  });
});
