import { type Static, t } from 'elysia';

export const TokensResponseDto = t.Object({
  access_token: t.String(),
  refresh_token: t.String(),
  token_type: t.String(),
  expires_in: t.Number(),
});

export type TokensResponseDtoType = Static<typeof TokensResponseDto>;

export const UserGroupAccessDto = t.Object({
  group_uid: t.String(),
  group_name: t.Nullable(t.String()),
  group_owner: t.Nullable(t.String()),
  group_logo: t.Nullable(t.String()),
  group_status: t.Nullable(t.String()),
  category: t.Nullable(t.String()),
  status: t.Nullable(t.String()),
  allow_admin: t.Boolean(),
  allow_patrol: t.Boolean(),
  allow_mon_patrol: t.Boolean(),
  features: t.Array(t.String()),
});

export type UserGroupAccessDtoType = Static<typeof UserGroupAccessDto>;

export const SafeUserDto = t.Object({
  id: t.String(),
  email: t.String(),
  name: t.String(),
  avatar_url: t.Nullable(t.String()),
  active_group: t.Nullable(UserGroupAccessDto),
  groups: t.Array(UserGroupAccessDto),
});

export type SafeUserDtoType = Static<typeof SafeUserDto>;

export const LoginResponseDto = t.Object({
  user: SafeUserDto,
  tokens: TokensResponseDto,
});

export type LoginResponseDtoType = Static<typeof LoginResponseDto>;

export const SwitchGroupResponseDto = LoginResponseDto;
export type SwitchGroupResponseDtoType = Static<typeof SwitchGroupResponseDto>;

export const LogoutResponseDto = t.Null();
export type LogoutResponseDtoType = Static<typeof LogoutResponseDto>;

export const JwksResponseDto = t.Object({
  keys: t.Array(t.Record(t.String(), t.Any())),
});

export type JwksResponseDtoType = Static<typeof JwksResponseDto>;
