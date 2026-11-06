import type { AuthenticatedUserPayload } from '@core/types/jwt-payload.types';
import type { GroupMemberItem } from '@features/group/domain/entities/group-member.entity';
import { GroupRequestMapper } from '@features/group/presentation/mappers/group-request.mapper';
import { GroupResponseMapper } from '@features/group/presentation/mappers/group-response.mapper';
import { describe, expect, it } from 'vitest';

describe('Group Mappers Unit Tests', () => {
  describe('GroupRequestMapper', () => {
    it('toGetGroupMembersInput harus membungkus currentUser secara tepat', () => {
      const user: AuthenticatedUserPayload = {
        userId: 'usr-123',
        role: 'ADMIN',
        group_uid: 'GRP-001',
      };

      const input = GroupRequestMapper.toGetGroupMembersInput(user);
      expect(input.currentUser).toBe(user);
      expect(input.currentUser.group_uid).toBe('GRP-001');
    });
  });

  describe('GroupResponseMapper', () => {
    it('toGroupMember harus memetakan entity ke response item DTO', () => {
      const item: GroupMemberItem = {
        email: 'personil@veranda.com',
        name: 'Personil Satu',
        profilePic: 'https://pic.jpg',
      };

      const dto = GroupResponseMapper.toGroupMember(item);
      expect(dto).toEqual({
        email: 'personil@veranda.com',
        name: 'Personil Satu',
        profilePic: 'https://pic.jpg',
      });
    });

    it('toGroupMemberList harus memetakan list entity ke list response item DTO', () => {
      const items: GroupMemberItem[] = [
        { email: 'a@v.com', name: 'A', profilePic: null },
        { email: 'b@v.com', name: null, profilePic: 'https://b.jpg' },
      ];

      const dtoList = GroupResponseMapper.toGroupMemberList(items);
      expect(dtoList).toHaveLength(2);
      expect(dtoList[0].email).toBe('a@v.com');
      expect(dtoList[1].email).toBe('b@v.com');
    });
  });
});
