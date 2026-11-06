import type { Failure } from '@core/error/failure';
import { Result } from '@core/error/result';
import { SessionVersionService } from '@core/security/session-version.service';
import { TokenService } from '@core/security/token.service';
import type { UseCase } from '@core/usecases/usecase.base';
import type { AuthRepository } from '@features/auth/domain/repositories/auth.repository';

export interface LogoutParams {
  refreshToken?: string;
  userId?: string;
}

export class LogoutUseCase implements UseCase<boolean, LogoutParams> {
  constructor(private readonly authRepository: AuthRepository) {}

  public async execute(params: LogoutParams): Promise<Result<boolean, Failure>> {
    if (params.refreshToken) {
      const tokenHash = TokenService.hashToken(params.refreshToken);
      await this.authRepository.revokeToken(tokenHash, 'LOGOUT', params.userId);
    }

    if (params.userId) {
      await SessionVersionService.incrementVersion(params.userId);
    }

    return Result.ok(true);
  }
}
