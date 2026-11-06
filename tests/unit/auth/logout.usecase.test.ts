import { TokenService } from '@core/security/token.service';
import type {
  AuthTokensEntity,
  RefreshTokenEntity,
} from '@features/auth/domain/entities/auth-tokens.entity';
import type { UserEntity } from '@features/auth/domain/entities/user.entity';
import type { AuthRepository } from '@features/auth/domain/repositories/auth.repository';
import { LogoutUseCase } from '@features/auth/usecases/logout.usecase';
import { describe, expect, it } from 'vitest';

class MockAuthRepository implements AuthRepository {
  public revokedTokens: Array<{ tokenHash: string; reason?: string; userId?: string }> = [];

  async findByEmail(_email: string, _groupUid?: string): Promise<UserEntity | null> {
    return null;
  }

  async findById(_id: string, _groupUid?: string): Promise<UserEntity | null> {
    return null;
  }

  async saveRefreshToken(_token: RefreshTokenEntity): Promise<void> {}

  async findRefreshTokenByHash(_tokenHash: string): Promise<RefreshTokenEntity | null> {
    return null;
  }

  async getRotatedFamilyId(_tokenHash: string): Promise<string | null> {
    return null;
  }

  async getGraceTokens(_tokenHash: string): Promise<AuthTokensEntity | null> {
    return null;
  }

  async rotateRefreshToken(_oldHash: string, _newToken: RefreshTokenEntity): Promise<boolean> {
    return true;
  }

  async revokeToken(tokenHash: string, reason?: string, userId?: string): Promise<void> {
    this.revokedTokens.push({ tokenHash, reason, userId });
  }

  async revokeTokenFamily(_familyId: string, _reason?: string): Promise<void> {}

  async acquireLock(_key: string, _lockValue: string, _ttlSeconds?: number): Promise<boolean> {
    return true;
  }

  async releaseLock(_key: string, _lockValue: string): Promise<void> {}
}

describe('LogoutUseCase Unit Tests', () => {
  it('berhasil mencabut refresh token di session store jika refresh_token disertakan', async () => {
    const mockRepo = new MockAuthRepository();
    const useCase = new LogoutUseCase(mockRepo);

    const rawRefreshToken = 'a'.repeat(64);
    const expectedHash = TokenService.hashToken(rawRefreshToken);

    const result = await useCase.execute({
      refreshToken: rawRefreshToken,
      userId: 'user-123',
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value).toBe(true);
    expect(mockRepo.revokedTokens).toHaveLength(1);
    expect(mockRepo.revokedTokens[0]?.tokenHash).toBe(expectedHash);
    expect(mockRepo.revokedTokens[0]?.reason).toBe('LOGOUT');
    expect(mockRepo.revokedTokens[0]?.userId).toBe('user-123');
  });

  it('berhasil logout tanpa mencabut token jika refresh_token tidak dikirim (misal client-side clear)', async () => {
    const mockRepo = new MockAuthRepository();
    const useCase = new LogoutUseCase(mockRepo);

    const result = await useCase.execute({
      userId: 'user-123',
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value).toBe(true);
    expect(mockRepo.revokedTokens).toHaveLength(0);
  });
});
