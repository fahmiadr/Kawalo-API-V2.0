import { AuthPrismaDataSource } from '@features/auth/data/datasources/auth-prisma.datasource';
import { AuthRedisDataSource } from '@features/auth/data/datasources/auth-redis.datasource';
import type {
  AuthTokensEntity,
  RefreshTokenEntity,
} from '@features/auth/domain/entities/auth-tokens.entity';
import type { UserEntity } from '@features/auth/domain/entities/user.entity';
import type { AuthRepository } from '@features/auth/domain/repositories/auth.repository';

export class AuthRepositoryImpl implements AuthRepository {
  constructor(
    private readonly prismaSource: AuthPrismaDataSource = new AuthPrismaDataSource(),
    private readonly redisSource: AuthRedisDataSource = new AuthRedisDataSource(),
  ) {}

  public async findByEmail(email: string, groupUid?: string | null): Promise<UserEntity | null> {
    return this.prismaSource.findByEmail(email, groupUid);
  }

  public async findById(id: string, groupUid?: string | null): Promise<UserEntity | null> {
    return this.prismaSource.findById(id, groupUid);
  }

  public async saveRefreshToken(token: RefreshTokenEntity): Promise<void> {
    await this.redisSource.saveSession(token);
  }

  public async findRefreshTokenByHash(tokenHash: string): Promise<RefreshTokenEntity | null> {
    return this.redisSource.getSession(tokenHash);
  }

  public async getRotatedFamilyId(tokenHash: string): Promise<string | null> {
    return this.redisSource.getRotatedFamilyId(tokenHash);
  }

  public async getGraceTokens(tokenHash: string): Promise<AuthTokensEntity | null> {
    return this.redisSource.getGraceTokens(tokenHash);
  }

  public async rotateRefreshToken(
    oldHash: string,
    newToken: RefreshTokenEntity,
    graceTokens?: AuthTokensEntity,
  ): Promise<boolean> {
    return this.redisSource.rotateSession(oldHash, newToken, graceTokens);
  }

  public async revokeToken(tokenHash: string, _reason?: string, userId?: string): Promise<void> {
    await this.redisSource.deleteSession(tokenHash, userId);
  }

  public async revokeTokenFamily(familyId: string, _reason?: string): Promise<void> {
    await this.redisSource.revokeFamily(familyId);
  }

  public async acquireLock(key: string, lockValue: string, ttlSeconds?: number): Promise<boolean> {
    return this.redisSource.acquireLock(key, lockValue, ttlSeconds);
  }

  public async releaseLock(key: string, lockValue: string): Promise<void> {
    await this.redisSource.releaseLock(key, lockValue);
  }
}
