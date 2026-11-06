import { type Static, t } from 'elysia';

export const LoginDto = t.Object({
  email: t.String({ format: 'email', description: 'Email personil kawalo' }),
  password: t.String({ minLength: 1, description: 'Kata sandi personil' }),
  group_uid: t.Optional(t.String({ description: 'UID grup aktif pilihan (opsional)' })),
});

export type LoginDtoType = Static<typeof LoginDto>;

export const SwitchGroupDto = t.Object({
  group_uid: t.String({ minLength: 1, description: 'Target Group UID yang akan diaktifkan' }),
});

export type SwitchGroupDtoType = Static<typeof SwitchGroupDto>;

export const RefreshTokenDto = t.Object({
  refresh_token: t.String({ minLength: 1, description: 'Raw refresh token (64 hex string)' }),
});

export type RefreshTokenDtoType = Static<typeof RefreshTokenDto>;

export const LogoutDto = t.Object({
  refresh_token: t.Optional(t.String({ description: 'Refresh token yang akan dicabut' })),
});

export type LogoutDtoType = Static<typeof LogoutDto>;
