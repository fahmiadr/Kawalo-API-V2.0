import { ErrorCodes } from '@core/constants/error.codes';
import { RsaKeyManager } from '@core/security/rsa-key.manager';
import { TokenService } from '@core/security/token.service';
import {
  AuthTokensEntity,
  RefreshTokenEntity,
} from '@features/auth/domain/entities/auth-tokens.entity';
import { UserEntity, UserGroupAccessEntity } from '@features/auth/domain/entities/user.entity';
import type { AuthRepository } from '@features/auth/domain/repositories/auth.repository';
import { RefreshTokenUseCase } from '@features/auth/usecases/refresh-token.usecase';
import { AuthResponseMapper } from '@features/auth/presentation/mappers/auth-response.mapper';
import { beforeEach, describe, expect, it, vi } from 'vitest';

class MockAuthRepository implements AuthRepository {
  public users: Map<string, UserEntity> = new Map();
  public refreshTokens: Map<string, RefreshTokenEntity> = new Map();
  public rotatedTokens: Map<string, string> = new Map();
  public graceTokens: Map<string, AuthTokensEntity> = new Map();

  async findByEmail(email: string, _groupUid?: string | null): Promise<UserEntity | null> {
    for (const u of this.users.values()) {
      if (u.email.toLowerCase() === email.toLowerCase()) return u;
    }
    return null;
  }

  async findById(id: string, _groupUid?: string | null): Promise<UserEntity | null> {
    return this.users.get(id) || null;
  }

  async saveRefreshToken(token: RefreshTokenEntity): Promise<void> {
    this.refreshTokens.set(token.tokenHash, token);
  }

  async findRefreshTokenByHash(tokenHash: string): Promise<RefreshTokenEntity | null> {
    return this.refreshTokens.get(tokenHash) || null;
  }

  async getRotatedFamilyId(tokenHash: string): Promise<string | null> {
    return this.rotatedTokens.get(tokenHash) || null;
  }

  async getGraceTokens(tokenHash: string): Promise<AuthTokensEntity | null> {
    return this.graceTokens.get(tokenHash) || null;
  }

  async rotateRefreshToken(
    oldHash: string,
    newToken: RefreshTokenEntity,
    graceTokens?: AuthTokensEntity,
  ): Promise<boolean> {
    const old = this.refreshTokens.get(oldHash);
    if (!old) return false;
    this.refreshTokens.delete(oldHash);
    this.rotatedTokens.set(oldHash, newToken.familyId);
    if (graceTokens) {
      this.graceTokens.set(oldHash, graceTokens);
    }
    this.refreshTokens.set(newToken.tokenHash, newToken);
    return true;
  }

  async revokeToken(tokenHash: string, _reason?: string): Promise<void> {
    this.refreshTokens.delete(tokenHash);
  }

  async revokeTokenFamily(familyId: string, _reason?: string): Promise<void> {
    for (const [hash, t] of this.refreshTokens.entries()) {
      if (t.familyId === familyId) {
        this.refreshTokens.delete(hash);
      }
    }
  }

  async acquireLock(_key: string, _lockValue?: string, _ttlSeconds?: number): Promise<boolean> {
    return true;
  }

  async releaseLock(_key: string, _lockValue?: string): Promise<void> {}
}

describe('RefreshTokenUseCase Unit Tests', () => {
  let mockRepo: MockAuthRepository;
  let useCase: RefreshTokenUseCase;

  const mockGroup = new UserGroupAccessEntity({
    groupUid: 'grp_alpha',
    groupName: 'Cluster Alpha',
    category: 'GUARD',
    status: 'JOINED',
    allowAdmin: false,
    allowPatrol: true,
    allowMonPatrol: false,
    features: ['PATROL'],
  });

  beforeEach(async () => {
    await RsaKeyManager.init();
    mockRepo = new MockAuthRepository();
    useCase = new RefreshTokenUseCase(mockRepo);

    mockRepo.users.set(
      '101',
      new UserEntity({
        id: '101',
        email: 'personil@kawalo.co.id',
        name: 'Test User',
        passwordHash: 'hash123',
        avatarUrl: null,
        activeGroup: mockGroup,
        groups: [mockGroup],
      }),
    );
  });

  it('harus berhasil merotasi token dengan valid token', async () => {
    const rawOldToken = TokenService.generateRefreshToken();
    const oldHash = TokenService.hashToken(rawOldToken);

    mockRepo.refreshTokens.set(
      oldHash,
      new RefreshTokenEntity({
        userId: '101',
        tokenHash: oldHash,
        familyId: 'family_1',
        expiresAt: new Date(Date.now() + 1000000),
      }),
    );

    const result = await useCase.execute({ refreshToken: rawOldToken });

    expect(result.isSuccess()).toBe(true);
    expect(result.value.accessToken).toBeDefined();
    expect(result.value.refreshToken).toBeDefined();
    expect(result.value.refreshToken).not.toBe(rawOldToken); // Token baru hasil rotasi

    // Verifikasi pemetaan Presentation via AuthResponseMapper
    const mapped = AuthResponseMapper.toRefreshResponse(result.value);
    expect(mapped.access_token).toBe(result.value.accessToken);
    expect(mapped.refresh_token).toBe(result.value.refreshToken);

    // Verifikasi activeGroupUid disinkronkan ke token baru
    const newHash = TokenService.hashToken(result.value.refreshToken);
    expect(mockRepo.refreshTokens.get(newHash)?.activeGroupUid).toBe('grp_alpha');
  });

  it('harus menolak pembaruan jika refresh token kosong', async () => {
    const resEmpty = await useCase.execute({ refreshToken: '' });
    expect(resEmpty.isFailure()).toBe(true);
    expect(resEmpty.failure.statusCode).toBe(400);
  });

  it('harus menolak pembaruan token jika status grup bukan JOINED', async () => {
    const inactiveGroup = new UserGroupAccessEntity({
      ...mockGroup,
      status: 'JOIN_WAIT',
    });
    mockRepo.users.set(
      '102',
      new UserEntity({
        id: '102',
        email: 'wait@kawalo.co.id',
        name: 'Wait User',
        passwordHash: 'hash123',
        avatarUrl: null,
        activeGroup: inactiveGroup,
        groups: [inactiveGroup],
      }),
    );

    const rawToken = TokenService.generateRefreshToken();
    const hash = TokenService.hashToken(rawToken);
    mockRepo.refreshTokens.set(
      hash,
      new RefreshTokenEntity({
        userId: '102',
        tokenHash: hash,
        familyId: 'family_2',
        expiresAt: new Date(Date.now() + 1000000),
      }),
    );

    const result = await useCase.execute({ refreshToken: rawToken });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.errorCode).toBe(ErrorCodes.AUTH_USER_INACTIVE);
    expect(result.failure.statusCode).toBe(403);
    expect(result.failure.message).toBe(
      'Akun Anda belum aktif atau belum disetujui. Silakan hubungi administrator Anda.',
    );
  });

  it('harus mengembalikan respons idempotent saat token lama dikirim kembali dalam masa grace period (15 detik)', async () => {
    const rawOldToken = TokenService.generateRefreshToken();
    const oldHash = TokenService.hashToken(rawOldToken);

    mockRepo.refreshTokens.set(
      oldHash,
      new RefreshTokenEntity({
        userId: '101',
        tokenHash: oldHash,
        familyId: 'family_grace',
        expiresAt: new Date(Date.now() + 1000000),
      }),
    );

    // Rotasi pertama
    const firstResult = await useCase.execute({ refreshToken: rawOldToken });
    expect(firstResult.isSuccess()).toBe(true);

    const revokeFamilySpy = vi.spyOn(mockRepo, 'revokeTokenFamily');

    // Pengiriman token lama kedua kalinya dalam grace period
    const secondResult = await useCase.execute({ refreshToken: rawOldToken });
    expect(secondResult.isSuccess()).toBe(true);
    expect(secondResult.value.accessToken).toBe(firstResult.value.accessToken);
    expect(secondResult.value.refreshToken).toBe(firstResult.value.refreshToken);

    // Tidak boleh mencabut token family karena masih dalam grace period
    expect(revokeFamilySpy).not.toHaveBeenCalled();
  });

  it('harus mendeteksi Reuse dan membatalkan family jika token yang sudah dirotasi dipakai lagi di luar masa grace period', async () => {
    const rawCompromisedToken = TokenService.generateRefreshToken();
    const compromisedHash = TokenService.hashToken(rawCompromisedToken);

    // Token sudah pernah dirotasi (tersimpan di tombstone, tanpa grace token)
    mockRepo.rotatedTokens.set(compromisedHash, 'family_compromised');

    const result = await useCase.execute({ refreshToken: rawCompromisedToken });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.errorCode).toBe(ErrorCodes.AUTH_REUSE_DETECTED);
    expect(result.failure.statusCode).toBe(401);
  });

  it('harus mencabut token beserta userId jika token telah kedaluwarsa', async () => {
    const rawToken = TokenService.generateRefreshToken();
    const hash = TokenService.hashToken(rawToken);
    mockRepo.refreshTokens.set(
      hash,
      new RefreshTokenEntity({
        userId: '101',
        tokenHash: hash,
        familyId: 'family_exp',
        expiresAt: new Date(Date.now() - 10000), // Sudah kedaluwarsa
      }),
    );

    const revokeSpy = vi.spyOn(mockRepo, 'revokeToken');

    const result = await useCase.execute({ refreshToken: rawToken });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.errorCode).toBe(ErrorCodes.AUTH_TOKEN_EXPIRED);
    expect(revokeSpy).toHaveBeenCalledWith(hash, 'EXPIRED', '101');
  });
});
