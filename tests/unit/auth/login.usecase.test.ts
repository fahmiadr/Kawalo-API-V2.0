import { EnvConfig } from '@core/config/env.config';
import { ErrorCodes } from '@core/constants/error.codes';
import { RsaKeyManager } from '@core/security/rsa-key.manager';
import { TokenService } from '@core/security/token.service';
import type {
  AuthTokensEntity,
  RefreshTokenEntity,
} from '@features/auth/domain/entities/auth-tokens.entity';
import { UserEntity, UserGroupAccessEntity } from '@features/auth/domain/entities/user.entity';
import type { AuthRepository } from '@features/auth/domain/repositories/auth.repository';
import { LoginUseCase } from '@features/auth/usecases/login.usecase';
import { AuthResponseMapper } from '@features/auth/presentation/mappers/auth-response.mapper';
import { beforeEach, describe, expect, it, vi } from 'vitest';
import { LoginRateLimiterService } from '@features/auth/infrastructure/login-rate-limiter.service';

class MockAuthRepository implements AuthRepository {
  public users: Map<string, UserEntity> = new Map();
  public refreshTokens: Map<string, RefreshTokenEntity> = new Map();
  public rotatedTokens: Map<string, string> = new Map();

  async findByEmail(email: string, groupUid?: string): Promise<UserEntity | null> {
    for (const u of this.users.values()) {
      if (u.email.toLowerCase() === email.toLowerCase()) {
        const requestedGroup = groupUid
          ? u.groups.find((g) => g.groupUid === groupUid) || null
          : null;
        const joinedGroup = u.groups.find((g) => g.status === 'JOINED') || null;
        const selectedGroup = groupUid ? requestedGroup : joinedGroup || u.activeGroup || null;
        return new UserEntity({
          id: u.id,
          email: u.email,
          name: u.name,
          passwordHash: u.passwordHash,
          avatarUrl: u.avatarUrl,
          activeGroup: selectedGroup,
          groups: u.groups,
        });
      }
    }
    return null;
  }

  async findById(id: string, groupUid?: string): Promise<UserEntity | null> {
    const u = this.users.get(id);
    if (!u) return null;
    if (groupUid) {
      const selectedGroup = u.groups.find((g) => g.groupUid === groupUid) || u.activeGroup;
      return new UserEntity({
        id: u.id,
        email: u.email,
        name: u.name,
        passwordHash: u.passwordHash,
        avatarUrl: u.avatarUrl,
        activeGroup: selectedGroup,
        groups: u.groups,
      });
    }
    return u;
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

  async getGraceTokens(_tokenHash: string): Promise<AuthTokensEntity | null> {
    return null;
  }

  async rotateRefreshToken(oldHash: string, newToken: RefreshTokenEntity): Promise<boolean> {
    const old = this.refreshTokens.get(oldHash);
    if (!old) return false;
    this.refreshTokens.delete(oldHash);
    this.rotatedTokens.set(oldHash, newToken.familyId);
    this.refreshTokens.set(newToken.tokenHash, newToken);
    return true;
  }

  async revokeToken(tokenHash: string, _reason?: string, _userId?: string): Promise<void> {
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

describe('LoginUseCase Unit Tests', () => {
  let mockRepo: MockAuthRepository;
  let useCase: LoginUseCase;

  const mockGroup = new UserGroupAccessEntity({
    groupUid: 'grp_alpha',
    groupName: 'Cluster Alpha',
    category: 'GUARD',
    status: 'JOINED',
    groupStatus: 'CLIENT',
    groupLogo: 'cluster_alpha_logo.png',
    allowAdmin: false,
    allowPatrol: true,
    allowMonPatrol: false,
    features: ['PATROL', 'GUEST'],
  });

  beforeEach(async () => {
    vi.clearAllMocks();
    vi.spyOn(LoginRateLimiterService, 'checkIpRateLimit').mockResolvedValue({
      isBlocked: false,
      remaining: 10,
      retryAfterSeconds: 0,
    });
    vi.spyOn(LoginRateLimiterService, 'checkEmailLock').mockResolvedValue({
      isLocked: false,
      remainingLockoutSeconds: 0,
    });
    vi.spyOn(LoginRateLimiterService, 'recordFailedAttempt').mockResolvedValue({
      isNowLocked: false,
      remainingAttempts: 4,
      lockoutDurationSeconds: 0,
    });
    vi.spyOn(LoginRateLimiterService, 'resetLoginAttempts').mockResolvedValue();

    await RsaKeyManager.init();
    mockRepo = new MockAuthRepository();
    useCase = new LoginUseCase(mockRepo);
  });

  it('harus gagal jika email tidak terdaftar (anti-enumeration)', async () => {
    const result = await useCase.execute({
      email: 'nonexistent@kawalo.co.id',
      password: 'password123',
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.statusCode).toBe(401);
    expect(result.failure.message).toBe('Email atau password yang Anda masukkan salah');
  });

  it('harus gagal jika password salah', async () => {
    mockRepo.users.set(
      '101',
      new UserEntity({
        id: '101',
        email: 'personil@kawalo.co.id',
        name: 'Personil Lapangan',
        passwordHash: 'rahasia123',
        avatarUrl: null,
        activeGroup: mockGroup,
        groups: [mockGroup],
      }),
    );

    const result = await useCase.execute({
      email: 'personil@kawalo.co.id',
      password: 'wrongPassword',
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.statusCode).toBe(401);
  });

  it('harus berhasil login dengan password plaintext dari tkuser dan klaim grup aktif', async () => {
    mockRepo.users.set(
      '101',
      new UserEntity({
        id: '101',
        email: 'personil@kawalo.co.id',
        name: 'Personil Lapangan',
        passwordHash: 'kawalo_pass',
        avatarUrl: 'avatar.jpg',
        activeGroup: mockGroup,
        groups: [mockGroup],
      }),
    );

    const result = await useCase.execute({
      email: 'personil@kawalo.co.id',
      password: 'kawalo_pass',
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value.user.email).toBe('personil@kawalo.co.id');
    expect(result.value.user.activeGroup?.groupName).toBe('Cluster Alpha');
    expect(result.value.user.activeGroup?.groupLogo).toBe('cluster_alpha_logo.png');
    expect(result.value.user.activeGroup?.category).toBe('GUARD');
    expect(result.value.user.activeGroup?.status).toBe('JOINED');
    expect(result.value.user.activeGroup?.allowAdmin).toBe(false);
    expect(result.value.user.activeGroup?.allowPatrol).toBe(true);
    expect(result.value.user.activeGroup?.allowMonPatrol).toBe(false);
    expect(result.value.tokens.accessToken).toBeDefined();
    expect(result.value.tokens.refreshToken).toBeDefined();
    expect(result.value.tokens.tokenType).toBe('Bearer');
    expect(result.value.tokens.expiresIn).toBe(EnvConfig.JWT_EXPIRES_IN);

    // Verifikasi AuthResponseMapper
    const mapped = AuthResponseMapper.toLoginResponse(result.value.user, result.value.tokens);
    expect(mapped.user.active_group?.group_name).toBe('Cluster Alpha');
    expect(mapped.tokens.access_token).toBe(result.value.tokens.accessToken);

    // Verifikasi klaim di dalam decoded RS256 JWT Access Token
    const decodedPayload = await TokenService.verifyAccessToken(result.value.tokens.accessToken);
    expect(decodedPayload).not.toBeNull();
    expect(decodedPayload?.sub).toBe('101');
    expect(decodedPayload?.email).toBe('personil@kawalo.co.id');
    expect(decodedPayload?.name).toBe('Personil Lapangan');
    expect(decodedPayload?.role).toBe('GUARD');
    expect(decodedPayload?.group_uid).toBe('grp_alpha');
    expect(decodedPayload?.allow_admin).toBe(false);
    expect(decodedPayload?.allow_patrol).toBe(true);
    expect(decodedPayload?.allow_mon_patrol).toBe(false);
  });

  it('harus menolak login jika status grup bukan JOINED (misal JOIN_WAIT) dengan pesan generic aman', async () => {
    const inactiveGroup = new UserGroupAccessEntity({
      ...mockGroup,
      groupUid: 'grp_wait',
      status: 'JOIN_WAIT',
    });
    mockRepo.users.set(
      '201',
      new UserEntity({
        id: '201',
        email: 'pending@kawalo.co.id',
        name: 'Personil Pending',
        passwordHash: 'password123',
        avatarUrl: null,
        activeGroup: inactiveGroup,
        groups: [inactiveGroup],
      }),
    );

    const result = await useCase.execute({
      email: 'pending@kawalo.co.id',
      password: 'password123',
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.errorCode).toBe(ErrorCodes.AUTH_USER_INACTIVE);
    expect(result.failure.statusCode).toBe(403);
    expect(result.failure.message).toBe(
      'Akun Anda belum aktif atau belum disetujui. Silakan hubungi administrator Anda.',
    );
  });

  it('harus memilih grup pertama yang berstatus JOINED jika user memiliki multi grup dan grup [0] berstatus JOIN_WAIT', async () => {
    const waitGroup = new UserGroupAccessEntity({
      ...mockGroup,
      groupUid: 'grp_wait_0',
      groupName: 'Cluster Wait',
      status: 'JOIN_WAIT',
      category: 'WAITING',
    });
    const joinedGroup = new UserGroupAccessEntity({
      ...mockGroup,
      groupUid: 'grp_joined_1',
      groupName: 'Cluster Joined',
      status: 'JOINED',
      category: 'GUARD',
    });

    mockRepo.users.set(
      '301',
      new UserEntity({
        id: '301',
        email: 'multigroup@kawalo.co.id',
        name: 'Personil Multi',
        passwordHash: 'password123',
        avatarUrl: null,
        activeGroup: null,
        groups: [waitGroup, joinedGroup],
      }),
    );

    const result = await useCase.execute({
      email: 'multigroup@kawalo.co.id',
      password: 'password123',
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value.user.activeGroup?.groupUid).toBe('grp_joined_1');
    expect(result.value.user.activeGroup?.status).toBe('JOINED');
    expect(result.value.user.activeGroup?.category).toBe('GUARD');
  });

  it('harus menolak login jika group_uid yang diminta bukan merupakan keanggotaan user (AUTH_FORBIDDEN)', async () => {
    mockRepo.users.set(
      '401',
      new UserEntity({
        id: '401',
        email: 'user@kawalo.co.id',
        name: 'Personil Single',
        passwordHash: 'pass123',
        avatarUrl: null,
        activeGroup: mockGroup,
        groups: [mockGroup],
      }),
    );

    const result = await useCase.execute({
      email: 'user@kawalo.co.id',
      password: 'pass123',
      groupUid: 'non_existent_group',
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.errorCode).toBe(ErrorCodes.AUTH_FORBIDDEN);
    expect(result.failure.statusCode).toBe(403);
    expect(result.failure.message).toBe(
      'Anda tidak terdaftar sebagai anggota pada grup yang dipilih',
    );
  });

  it('harus berhasil login dan mengaktifkan grup jika group_uid yang diminta valid dan JOINED', async () => {
    const secondGroup = new UserGroupAccessEntity({
      ...mockGroup,
      groupUid: 'grp_bravo',
      groupName: 'Cluster Bravo',
      status: 'JOINED',
      category: 'ADMIN',
    });

    mockRepo.users.set(
      '501',
      new UserEntity({
        id: '501',
        email: 'admin@kawalo.co.id',
        name: 'Personil Dua Grup',
        passwordHash: 'adminPass',
        avatarUrl: null,
        activeGroup: mockGroup,
        groups: [mockGroup, secondGroup],
      }),
    );

    const result = await useCase.execute({
      email: 'admin@kawalo.co.id',
      password: 'adminPass',
      groupUid: 'grp_bravo',
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value.user.activeGroup?.groupUid).toBe('grp_bravo');
    expect(result.value.user.activeGroup?.groupName).toBe('Cluster Bravo');
    expect(result.value.user.activeGroup?.category).toBe('ADMIN');
  });

  it('harus memblokir login jika IP melampaui ambang batas rate limit (Layer 1)', async () => {
    vi.spyOn(LoginRateLimiterService, 'checkIpRateLimit').mockResolvedValueOnce({
      isBlocked: true,
      remaining: 0,
      retryAfterSeconds: 900,
    });

    const result = await useCase.execute({
      email: 'personil@kawalo.co.id',
      password: 'password123',
      clientIp: '192.168.1.10',
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.errorCode).toBe(ErrorCodes.RATE_LIMIT_EXCEEDED);
    expect(result.failure.statusCode).toBe(429);
    expect(result.failure.retryAfterSeconds).toBe(900);
  });

  it('harus memblokir login jika akun sedang dikunci akibat gagal berulang (Layer 2)', async () => {
    vi.spyOn(LoginRateLimiterService, 'checkEmailLock').mockResolvedValueOnce({
      isLocked: true,
      remainingLockoutSeconds: 600,
    });

    const result = await useCase.execute({
      email: 'personil@kawalo.co.id',
      password: 'password123',
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.errorCode).toBe(ErrorCodes.AUTH_ACCOUNT_LOCKED);
    expect(result.failure.statusCode).toBe(429);
    expect(result.failure.retryAfterSeconds).toBe(600);
  });
});
