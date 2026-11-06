import { RsaKeyManager } from '@core/security/rsa-key.manager';
import type { AuthenticatedHandlerContext, BaseHandlerContext } from '@core/types/context.types';
import type { JwtUserPayload } from '@core/types/jwt-payload.types';
import { ApiResponse } from '@core/utils/api-response';
import { resolveClientIp } from '@core/utils/client-ip.util';
import type {
  LoginDtoType,
  LogoutDtoType,
  RefreshTokenDtoType,
  SwitchGroupDtoType,
} from '@features/auth/presentation/dtos/auth-request.dto';
import { AuthRequestMapper } from '@features/auth/presentation/mappers/auth-request.mapper';
import { AuthResponseMapper } from '@features/auth/presentation/mappers/auth-response.mapper';
import type { LoginUseCase } from '@features/auth/usecases/login.usecase';
import type { LogoutUseCase } from '@features/auth/usecases/logout.usecase';
import type { RefreshTokenUseCase } from '@features/auth/usecases/refresh-token.usecase';
import type { SwitchGroupUseCase } from '@features/auth/usecases/switch-group.usecase';

export interface LoginContext extends BaseHandlerContext {
  body: LoginDtoType;
  request?: Request;
  clientIp?: string;
}

export interface RefreshContext extends BaseHandlerContext {
  body: RefreshTokenDtoType;
}

export interface SwitchGroupContext extends AuthenticatedHandlerContext {
  body: SwitchGroupDtoType;
}

export interface LogoutContext extends BaseHandlerContext {
  currentUser?: JwtUserPayload | null;
  body?: LogoutDtoType;
}

export type JwksContext = BaseHandlerContext;

export class AuthHandler {
  constructor(
    private readonly loginUseCase: LoginUseCase,
    private readonly refreshTokenUseCase: RefreshTokenUseCase,
    private readonly switchGroupUseCase: SwitchGroupUseCase,
    private readonly logoutUseCase: LogoutUseCase,
  ) {}

  public async login({ body, set, request, clientIp }: LoginContext) {
    const resolvedIp = clientIp || (request ? resolveClientIp(request) : undefined);
    const input = AuthRequestMapper.toLoginInput(body, resolvedIp);
    const result = await this.loginUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      if (result.failure.retryAfterSeconds) {
        set.headers = set.headers ?? {};
        set.headers['Retry-After'] = String(result.failure.retryAfterSeconds);
      }
      return ApiResponse.fromFailure(result.failure);
    }

    const responseData = AuthResponseMapper.toLoginResponse(result.value.user, result.value.tokens);
    return ApiResponse.success(responseData, 'Login berhasil');
  }

  public async refresh({ body, set }: RefreshContext) {
    const input = AuthRequestMapper.toRefreshInput(body);
    const result = await this.refreshTokenUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    const responseData = AuthResponseMapper.toRefreshResponse(result.value);
    return ApiResponse.success(responseData, 'Token berhasil diperbarui');
  }

  public async switchGroup({ body, currentUser, set }: SwitchGroupContext) {
    const input = AuthRequestMapper.toSwitchGroupInput(body, currentUser);
    const result = await this.switchGroupUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    const responseData = AuthResponseMapper.toLoginResponse(result.value.user, result.value.tokens);
    return ApiResponse.success(responseData, 'Grup aktif berhasil diperbarui');
  }

  public async logout({ body, currentUser, set }: LogoutContext) {
    const input = AuthRequestMapper.toLogoutInput(body, currentUser);
    const result = await this.logoutUseCase.execute(input);

    if (result.isFailure()) {
      set.status = result.failure.status;
      return ApiResponse.fromFailure(result.failure);
    }

    return ApiResponse.success(null, 'Logout berhasil');
  }

  public async jwks({ set }: JwksContext) {
    set.headers = set.headers ?? {};
    set.headers['Cache-Control'] = 'public, max-age=86400';
    return RsaKeyManager.getJwks();
  }
}
