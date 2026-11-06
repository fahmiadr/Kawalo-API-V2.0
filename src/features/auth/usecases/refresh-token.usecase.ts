import { randomUUID } from 'node:crypto';
import { AppConstants } from '@core/constants/app.constants';
import { ErrorCodes } from '@core/constants/error.codes';
import { AuthFailure, type Failure, RateLimitFailure } from '@core/error/failure';
import { Result } from '@core/error/result';
import { SessionVersionService } from '@core/security/session-version.service';
import { TokenService } from '@core/security/token.service';
import type { UseCase } from '@core/usecases/usecase.base';
import { AuthConfig } from '@features/auth/config/auth.config';
import {
  AuthTokensEntity,
  RefreshTokenEntity,
} from '@features/auth/domain/entities/auth-tokens.entity';
import type { AuthRepository } from '@features/auth/domain/repositories/auth.repository';

export interface RefreshTokenParams {
  refreshToken?: string;
}

export class RefreshTokenUseCase implements UseCase<AuthTokensEntity, RefreshTokenParams> {
  constructor(private readonly authRepository: AuthRepository) {}

  public async execute(params: RefreshTokenParams): Promise<Result<AuthTokensEntity, Failure>> {
    // 1. Domain Input Guard: Validasi ketersediaan parameter tanpa mutasi atau sanitasi
    if (!params.refreshToken) {
      return Result.fail(
        new AuthFailure(
          'Refresh token wajib disertakan',
          ErrorCodes.AUTH_TOKEN_INVALID,
          'Bad Request',
        ),
      );
    }

    const tokenHash = TokenService.hashToken(params.refreshToken);
    const lockValue = randomUUID();

    // 2. Cegah race condition via distributed lock
    const lockAcquired = await this.authRepository.acquireLock(tokenHash, lockValue, 5);
    if (!lockAcquired) {
      return Result.fail(
        new RateLimitFailure(
          'Permintaan pembaruan sesi sedang diproses. Silakan coba beberapa saat lagi.',
        ),
      );
    }

    try {
      // 3. Cari sesi aktif di Redis
      const existingToken = await this.authRepository.findRefreshTokenByHash(tokenHash);

      if (!existingToken) {
        // 3.1 Idempotent Grace Period Check (15 detik untuk toleransi race condition / latency seluler)
        const graceTokens = await this.authRepository.getGraceTokens(tokenHash);
        if (graceTokens) {
          return Result.ok(graceTokens);
        }

        // 4. Deteksi Token Reuse via Tombstone Family Tracking (RFC 6749 Section 10.4)
        const rotatedFamilyId = await this.authRepository.getRotatedFamilyId(tokenHash);

        if (rotatedFamilyId) {
          console.warn(
            `[Security Alert] Refresh token reuse detected for family ${rotatedFamilyId}!`,
          );

          // Revoke seluruh family token yang bocor demi keamanan
          await this.authRepository.revokeTokenFamily(rotatedFamilyId, 'REUSE_DETECTED');

          return Result.fail(
            new AuthFailure(
              'Sesi Anda dihentikan demi keamanan karena terdeteksi anomali token. Silakan login kembali.',
              ErrorCodes.AUTH_REUSE_DETECTED,
              'Unauthorized',
            ),
          );
        }

        return Result.fail(
          new AuthFailure(
            'Sesi tidak ditemukan atau telah kedaluwarsa',
            ErrorCodes.AUTH_TOKEN_INVALID,
            'Unauthorized',
          ),
        );
      }

      // 5. Validasi masa berlaku token
      if (new Date() > existingToken.expiresAt) {
        await this.authRepository.revokeToken(tokenHash, 'EXPIRED', existingToken.userId);
        return Result.fail(
          new AuthFailure(
            'Sesi telah kedaluwarsa. Silakan login kembali.',
            ErrorCodes.AUTH_TOKEN_EXPIRED,
            'Unauthorized',
          ),
        );
      }

      // 6. Ambil data personil & verifikasi keanggotaan aktif (Pure Nullable groupUid)
      const user = await this.authRepository.findById(
        existingToken.userId,
        existingToken.activeGroupUid,
      );
      if (!user) {
        return Result.fail(
          new AuthFailure(
            'Pengguna tidak ditemukan',
            ErrorCodes.AUTH_USER_NOT_FOUND,
            'Unauthorized',
          ),
        );
      }

      if (user.activeGroup?.status !== 'JOINED') {
        return Result.fail(
          new AuthFailure(
            'Akun Anda belum aktif atau belum disetujui. Silakan hubungi administrator Anda.',
            ErrorCodes.AUTH_USER_INACTIVE,
            'Forbidden',
          ),
        );
      }

      // 7. Terbitkan Access Token baru dengan klaim grup aktif & versi sesi terkini
      const sessionVer = await SessionVersionService.getOrInitVersion(user.id);
      const { token: newAccessToken, expiresIn } = await TokenService.signAccessToken({
        sub: user.id,
        email: user.email,
        name: user.name,
        role: user.activeGroup.category,
        group_uid: user.activeGroup.groupUid,
        allow_admin: user.activeGroup.allowAdmin,
        allow_patrol: user.activeGroup.allowPatrol,
        allow_mon_patrol: user.activeGroup.allowMonPatrol,
        session_ver: sessionVer,
      });

      // 8. Terbitkan Refresh Token baru dalam Family ID yang sama & sinkronkan activeGroupUid
      const newRawRefreshToken = TokenService.generateRefreshToken();
      const newTokenHash = TokenService.hashToken(newRawRefreshToken);
      const newExpiresAt = AuthConfig.calculateRefreshTokenExpiry();

      const newTokenEntity = new RefreshTokenEntity({
        userId: user.id,
        tokenHash: newTokenHash,
        familyId: existingToken.familyId,
        expiresAt: newExpiresAt,
        activeGroupUid: user.activeGroup.groupUid,
      });

      const tokens = new AuthTokensEntity({
        accessToken: newAccessToken,
        refreshToken: newRawRefreshToken,
        tokenType: AppConstants.AUTH_TOKEN_TYPE,
        expiresIn,
      });

      // 9. Lakukan rotasi atomik di Redis dengan pencatatan Idempotent Grace Period
      const rotated = await this.authRepository.rotateRefreshToken(
        tokenHash,
        newTokenEntity,
        tokens,
      );
      if (!rotated) {
        return Result.fail(
          new AuthFailure(
            'Gagal melakukan rotasi token sesi',
            ErrorCodes.AUTH_TOKEN_INVALID,
            'Unauthorized',
          ),
        );
      }

      return Result.ok(tokens);
    } finally {
      await this.authRepository.releaseLock(tokenHash, lockValue);
    }
  }
}
