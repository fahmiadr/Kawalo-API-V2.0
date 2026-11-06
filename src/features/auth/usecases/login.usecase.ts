import { AppConstants } from '@core/constants/app.constants';
import { ErrorCodes } from '@core/constants/error.codes';
import { AuthFailure, Failure } from '@core/error/failure';
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
import { LoginRateLimiterService } from '@features/auth/infrastructure/login-rate-limiter.service';
import crypto from 'crypto';

export interface LoginParams {
  email?: string;
  password?: string;
  groupUid?: string;
  clientIp?: string;
}

export interface LoginResult {
  user: UserEntity;
  tokens: AuthTokensEntity;
}

export class LoginUseCase implements UseCase<LoginResult, LoginParams> {
  constructor(private readonly authRepository: AuthRepository) {}

  public async execute(params: LoginParams): Promise<Result<LoginResult, Failure>> {
    if (!params.email || !params.password) {
      return Result.fail(new AuthFailure());
    }

    // 1. Layer 1 Check: Batas frekuensi IP (IP Throttling)
    if (params.clientIp) {
      const ipStatus = await LoginRateLimiterService.checkIpRateLimit(params.clientIp);
      if (ipStatus.isBlocked) {
        const retryMinutes = Math.ceil(ipStatus.retryAfterSeconds / 60);
        return Result.fail(
          Failure.rateLimit(
            ipStatus.retryAfterSeconds,
            `Terlalu banyak permintaan login dari IP ini. Silakan coba kembali dalam ${retryMinutes} menit.`,
          ),
        );
      }
    }

    // 2. Layer 2 Check: Status Penguncian Akun (Account Lockout)
    const lockStatus = await LoginRateLimiterService.checkEmailLock(params.email);
    if (lockStatus.isLocked) {
      return Result.fail(Failure.locked(lockStatus.remainingLockoutSeconds));
    }

    const user = await this.authRepository.findByEmail(params.email, params.groupUid);

    // Anti-enumeration: generic error message if user not found, catat kegagalan
    if (!user) {
      await LoginRateLimiterService.recordFailedAttempt(params.email, params.clientIp);
      return Result.fail(new AuthFailure());
    }

    // CATATAN ARSITEKTUR & ROADMAP KEAMANAN:
    // Skema basis data warisan MariaDB (tabel `tkuser.Password` VARCHAR(64)) saat ini masih menyimpan
    // kata sandi dalam format plaintext. Untuk menjaga kompatibilitas penuh dengan sistem eksisting,
    // verifikasi dilakukan secara langsung (direct comparison).
    // Rencana Modernisasi: Akan diterapkan transparent re-hashing (Argon2id/Bcrypt) saat pengguna
    // berhasil login di masa mendatang tanpa mengganggu alur masuk personil aktif.
    if (params.password !== user.passwordHash) {
      await LoginRateLimiterService.recordFailedAttempt(params.email, params.clientIp);
      return Result.fail(new AuthFailure());
    }

    // Login kredensial valid: reset riwayat kegagalan akun
    await LoginRateLimiterService.resetLoginAttempts(params.email);

    // Jika personil meminta grup spesifik, pastikan personil memang terdaftar pada grup tersebut
    if (params.groupUid && !user.groups.some((g) => g.groupUid === params.groupUid)) {
      return Result.fail(
        new AuthFailure(
          'Anda tidak terdaftar sebagai anggota pada grup yang dipilih',
          ErrorCodes.AUTH_FORBIDDEN,
          'Forbidden',
        ),
      );
    }

    // Hanya keanggotaan dengan status JOINED yang diizinkan login
    if (user.activeGroup?.status !== 'JOINED') {
      return Result.fail(
        new AuthFailure(
          'Akun Anda belum aktif atau belum disetujui. Silakan hubungi administrator Anda.',
          ErrorCodes.AUTH_USER_INACTIVE,
          'Forbidden',
        ),
      );
    }

    // Issue RS256 Signed JWT Access Token with Active Group Claims & Session Version
    const sessionVer = await SessionVersionService.getOrInitVersion(user.id);
    const { token: accessToken, expiresIn } = await TokenService.signAccessToken({
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

    // Issue Opaque CSPRNG Refresh Token
    const rawRefreshToken = TokenService.generateRefreshToken();
    const tokenHash = TokenService.hashToken(rawRefreshToken);
    const familyId = crypto.randomUUID();
    const expiresAt = AuthConfig.calculateRefreshTokenExpiry();

    const refreshTokenEntity = new RefreshTokenEntity({
      userId: user.id,
      tokenHash,
      familyId,
      expiresAt,
      activeGroupUid: user.activeGroup?.groupUid ?? null,
    });

    // Save session to Redis
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
