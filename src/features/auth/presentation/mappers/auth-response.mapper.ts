import type { AuthTokensEntity } from '@features/auth/domain/entities/auth-tokens.entity';
import type { UserEntity, UserGroupAccessEntity } from '@features/auth/domain/entities/user.entity';
import type {
  LoginResponseDtoType,
  SafeUserDtoType,
  TokensResponseDtoType,
  UserGroupAccessDtoType,
} from '@features/auth/presentation/dtos/auth-response.dto';

export class AuthResponseMapper {
  public static toGroupAccessResponse<T extends UserGroupAccessEntity | null>(
    group: T,
  ): T extends UserGroupAccessEntity ? UserGroupAccessDtoType : null;
  public static toGroupAccessResponse(
    group: UserGroupAccessEntity | null,
  ): UserGroupAccessDtoType | null {
    if (!group) return null;

    return {
      group_uid: group.groupUid,
      group_name: group.groupName,
      group_owner: group.groupOwner,
      group_logo: group.groupLogo,
      group_status: group.groupStatus,
      category: group.category,
      status: group.status,
      allow_admin: group.allowAdmin,
      allow_patrol: group.allowPatrol,
      allow_mon_patrol: group.allowMonPatrol,
      features: group.features,
    };
  }

  public static toUserResponse(user: UserEntity): SafeUserDtoType {
    return {
      id: user.id,
      email: user.email,
      name: user.name,
      avatar_url: user.avatarUrl,
      active_group: AuthResponseMapper.toGroupAccessResponse(user.activeGroup),
      groups: user.groups.map(AuthResponseMapper.toGroupAccessResponse),
    };
  }

  public static toTokensResponse(tokens: AuthTokensEntity): TokensResponseDtoType {
    return {
      access_token: tokens.accessToken,
      refresh_token: tokens.refreshToken,
      token_type: tokens.tokenType,
      expires_in: tokens.expiresIn,
    };
  }

  public static toLoginResponse(user: UserEntity, tokens: AuthTokensEntity): LoginResponseDtoType {
    return {
      user: AuthResponseMapper.toUserResponse(user),
      tokens: AuthResponseMapper.toTokensResponse(tokens),
    };
  }

  public static toRefreshResponse(tokens: AuthTokensEntity): TokensResponseDtoType {
    return AuthResponseMapper.toTokensResponse(tokens);
  }
}
