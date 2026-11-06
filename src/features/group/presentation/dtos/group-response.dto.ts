import { type Static, t } from 'elysia';

export const GroupMemberItemDto = t.Object({
  email: t.String({ description: 'Email anggota dalam grup aktif' }),
  name: t.Nullable(t.String({ description: 'Nama anggota' })),
  profilePic: t.Nullable(t.String({ description: 'URL foto profil anggota' })),
});

export type GroupMemberItemDtoType = Static<typeof GroupMemberItemDto>;

export const GetGroupMembersResponseDto = t.Array(GroupMemberItemDto);
export type GetGroupMembersResponseDtoType = Static<typeof GetGroupMembersResponseDto>;
