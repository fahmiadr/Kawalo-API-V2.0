import type {
  AuthTokensEntity,
  RefreshTokenEntity,
} from '@features/auth/domain/entities/auth-tokens.entity';
import type { UserEntity } from '@features/auth/domain/entities/user.entity';

export interface AuthRepository {
  findByEmail(email: string, groupUid?: string | null): Promise<UserEntity | null>;
  findById(id: string, groupUid?: string | null): Promise<UserEntity | null>;

  saveRefreshToken(token: RefreshTokenEntity): Promise<void>;
  findRefreshTokenByHash(tokenHash: string): Promise<RefreshTokenEntity | null>;
  getRotatedFamilyId(tokenHash: string): Promise<string | null>;
  getGraceTokens(tokenHash: string): Promise<AuthTokensEntity | null>;
  rotateRefreshToken(
    oldHash: string,
    newToken: RefreshTokenEntity,
    graceTokens?: AuthTokensEntity,
  ): Promise<boolean>;
  revokeToken(tokenHash: string, reason?: string, userId?: string): Promise<void>;
  revokeTokenFamily(familyId: string, reason?: string): Promise<void>;

  acquireLock(key: string, lockValue: string, ttlSeconds?: number): Promise<boolean>;
  releaseLock(key: string, lockValue: string): Promise<void>;
}
