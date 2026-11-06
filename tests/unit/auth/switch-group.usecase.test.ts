import { ErrorCodes } from '@core/constants/error.codes';
import { RsaKeyManager } from '@core/security/rsa-key.manager';
import { TokenService } from '@core/security/token.service';
import type {
  AuthTokensEntity,
  RefreshTokenEntity,
} from '@features/auth/domain/entities/auth-tokens.entity';
import { UserEntity, UserGroupAccessEntity } from '@features/auth/domain/entities/user.entity';
import type { AuthRepository } from '@features/auth/domain/repositories/auth.repository';
import { SwitchGroupUseCase } from '@features/auth/usecases/switch-group.usecase';
import { AuthResponseMapper } from '@features/auth/presentation/mappers/auth-response.mapper';
import { beforeEach, describe, expect, it } from 'vitest';

class MockAuthRepository implements AuthRepository {
  public users: Map<string, UserEntity> = new Map();
  public refreshTokens: Map<string, RefreshTokenEntity> = new Map();

  async findByEmail(_email: string, _groupUid?: string): Promise<UserEntity | null> {
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

  async getRotatedFamilyId(_tokenHash: string): Promise<string | null> {
    return null;
  }

  async getGraceTokens(_tokenHash: string): Promise<AuthTokensEntity | null> {
    return null;
  }

  async rotateRefreshToken(_oldHash: string, _newToken: RefreshTokenEntity): Promise<boolean> {
    return true;
  }

  async revokeToken(tokenHash: string, _reason?: string, _userId?: string): Promise<void> {
    this.refreshTokens.delete(tokenHash);
  }

  async revokeTokenFamily(_familyId: string, _reason?: string): Promise<void> {
    this.refreshTokens.clear();
  }

  async acquireLock(_key: string, _lockValue?: string, _ttlSeconds?: number): Promise<boolean> {
    return true;
  }

  async releaseLock(_key: string, _lockValue?: string): Promise<void> {}
}

describe('SwitchGroupUseCase Unit Tests', () => {
  let mockRepo: MockAuthRepository;
  let useCase: SwitchGroupUseCase;

  const joinedGroup1 = new UserGroupAccessEntity({
    groupUid: 'grp_1',
    groupName: 'Group Alpha',
    category: 'GUARD',
    status: 'JOINED',
    allowAdmin: false,
    allowPatrol: true,
    allowMonPatrol: false,
    features: ['PATROL'],
  });

  const joinedGroup2 = new UserGroupAccessEntity({
    groupUid: 'grp_2',
    groupName: 'Group Beta',
    category: 'ADMIN',
    status: 'JOINED',
    allowAdmin: true,
    allowPatrol: true,
    allowMonPatrol: true,
    features: ['PATROL', 'ADMIN'],
  });

  const pendingGroup = new UserGroupAccessEntity({
    groupUid: 'grp_pending',
    groupName: 'Group Pending',
    category: 'WAITING',
    status: 'JOIN_WAIT',
    allowAdmin: false,
    allowPatrol: false,
    allowMonPatrol: false,
    features: [],
  });

  beforeEach(async () => {
    await RsaKeyManager.init();
    mockRepo = new MockAuthRepository();
    useCase = new SwitchGroupUseCase(mockRepo);

    mockRepo.users.set(
      '101',
      new UserEntity({
        id: '101',
        email: 'personil@kawalo.co.id',
        name: 'Test User',
        passwordHash: 'hash123',
        avatarUrl: null,
        activeGroup: joinedGroup1,
        groups: [joinedGroup1, joinedGroup2, pendingGroup],
      }),
    );
  });

  it('harus berhasil berpindah ke grup lain yang berstatus JOINED', async () => {
    const result = await useCase.execute({
      userId: '101',
      groupUid: 'grp_2',
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value.user.activeGroup?.groupUid).toBe('grp_2');
    expect(result.value.user.activeGroup?.category).toBe('ADMIN');
    expect(result.value.user.activeGroup?.allowAdmin).toBe(true);
    expect(result.value.user.activeGroup?.allowPatrol).toBe(true);
    expect(result.value.user.activeGroup?.allowMonPatrol).toBe(true);

    // Verifikasi AuthResponseMapper
    const mapped = AuthResponseMapper.toLoginResponse(result.value.user, result.value.tokens);
    expect(mapped.user.active_group?.group_uid).toBe('grp_2');
    expect(mapped.tokens.access_token).toBe(result.value.tokens.accessToken);

    const decoded = await TokenService.verifyAccessToken(result.value.tokens.accessToken);
    expect(decoded?.group_uid).toBe('grp_2');
    expect(decoded?.role).toBe('ADMIN');
    expect(decoded?.allow_admin).toBe(true);
    expect(decoded?.allow_patrol).toBe(true);
    expect(decoded?.allow_mon_patrol).toBe(true);
  });

  it('harus menolak perpindahan grup jika status grup target bukan JOINED', async () => {
    const result = await useCase.execute({
      userId: '101',
      groupUid: 'grp_pending',
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.errorCode).toBe(ErrorCodes.AUTH_USER_INACTIVE);
    expect(result.failure.statusCode).toBe(403);
    expect(result.failure.message).toBe(
      'Akun Anda belum aktif atau belum disetujui. Silakan hubungi administrator Anda.',
    );
  });

  it('harus menolak perpindahan grup jika pengguna bukan anggota grup tersebut', async () => {
    const result = await useCase.execute({
      userId: '101',
      groupUid: 'grp_unknown',
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.errorCode).toBe(ErrorCodes.AUTH_FORBIDDEN);
    expect(result.failure.statusCode).toBe(403);
  });
});
