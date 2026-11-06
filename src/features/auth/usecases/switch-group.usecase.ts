import { AppConstants } from '@core/constants/app.constants';
import { ErrorCodes } from '@core/constants/error.codes';
import { AuthFailure, type Failure } from '@core/error/failure';
import { Result } from '@core/error/result';
import { SessionVersionService } from '@core/security/session-version.service';
import { TokenService } from '@core/security/token.service';
import type { UseCase } from '@core/usecases/usecase.base';
import { AuthConfig } from '@features/auth/config/auth.config';
import {
  AuthTokensEntity,
  RefreshTokenEntity,
} from '@features/auth/domain/entities/auth-tokens.entity';
import type { UserEntity } from '@features/auth/domain/entities/user.entity';
import type { AuthRepository } from '@features/auth/domain/repositories/auth.repository';
import crypto from 'crypto';

export interface SwitchGroupParams {
  userId?: string;
  groupUid?: string;
}

export interface SwitchGroupResult {
  user: UserEntity;
  tokens: AuthTokensEntity;
}

export class SwitchGroupUseCase implements UseCase<SwitchGroupResult, SwitchGroupParams> {
  constructor(private readonly authRepository: AuthRepository) {}

  public async execute(params: SwitchGroupParams): Promise<Result<SwitchGroupResult, Failure>> {
    if (!params.groupUid) {
      return Result.fail(
        new AuthFailure('Group UID wajib disertakan', ErrorCodes.VALIDATION_ERROR, 'Bad Request'),
      );
    }

    if (!params.userId) {
      return Result.fail(
        new AuthFailure('Pengguna tidak ditemukan', ErrorCodes.AUTH_USER_NOT_FOUND, 'Unauthorized'),
      );
    }

    const user = await this.authRepository.findById(params.userId, params.groupUid);
    if (!user) {
      return Result.fail(
        new AuthFailure('Pengguna tidak ditemukan', ErrorCodes.AUTH_USER_NOT_FOUND, 'Unauthorized'),
      );
    }

    // Verify membership in requested group
    const targetGroup = user.groups.find((g) => g.groupUid === params.groupUid);
    if (!targetGroup) {
      return Result.fail(
        new AuthFailure(
          'Anda tidak terdaftar sebagai anggota pada grup yang dipilih',
          ErrorCodes.AUTH_FORBIDDEN,
          'Forbidden',
        ),
      );
    }

    // Hanya keanggotaan dengan status JOINED yang dapat dipilih
    if (targetGroup.status !== 'JOINED') {
      return Result.fail(
        new AuthFailure(
          'Akun Anda belum aktif atau belum disetujui. Silakan hubungi administrator Anda.',
          ErrorCodes.AUTH_USER_INACTIVE,
          'Forbidden',
        ),
      );
    }

    // Issue RS256 Signed JWT Access Token for the newly active group & Session Version
    const sessionVer = await SessionVersionService.getOrInitVersion(user.id);
    const { token: accessToken, expiresIn } = await TokenService.signAccessToken({
      sub: user.id,
      email: user.email,
      name: user.name,
      role: targetGroup.category ?? null,
      group_uid: targetGroup.groupUid,
      allow_admin: targetGroup.allowAdmin,
      allow_patrol: targetGroup.allowPatrol,
      allow_mon_patrol: targetGroup.allowMonPatrol,
      session_ver: sessionVer,
    });

    // Issue new Refresh Token
    const rawRefreshToken = TokenService.generateRefreshToken();
    const tokenHash = TokenService.hashToken(rawRefreshToken);
    const familyId = crypto.randomUUID();
    const expiresAt = AuthConfig.calculateRefreshTokenExpiry();

    const refreshTokenEntity = new RefreshTokenEntity({
      userId: user.id,
      tokenHash,
      familyId,
      expiresAt,
      activeGroupUid: params.groupUid,
    });

    await this.authRepository.saveRefreshToken(refreshTokenEntity);

    const tokens = new AuthTokensEntity({
      accessToken,
      refreshToken: rawRefreshToken,
      tokenType: AppConstants.AUTH_TOKEN_TYPE,
      expiresIn,
    });

    return Result.ok({
      user,
      tokens,
    });
  }
}
