import type { AuthenticatedUserPayload, JwtUserPayload } from '@core/types/jwt-payload.types';
import { cleanQueryString } from '@core/utils/clean-data.util';
import type {
  LoginDtoType,
  LogoutDtoType,
  RefreshTokenDtoType,
  SwitchGroupDtoType,
} from '@features/auth/presentation/dtos/auth-request.dto';
import type { LoginParams } from '@features/auth/usecases/login.usecase';
import type { LogoutParams } from '@features/auth/usecases/logout.usecase';
import type { RefreshTokenParams } from '@features/auth/usecases/refresh-token.usecase';
import type { SwitchGroupParams } from '@features/auth/usecases/switch-group.usecase';

export class AuthRequestMapper {
  public static toLoginInput(body: LoginDtoType, clientIp?: string): LoginParams {
    return {
      email: cleanQueryString(body.email),
      password: cleanQueryString(body.password),
      groupUid: cleanQueryString(body.group_uid),
      clientIp,
    };
  }

  public static toRefreshInput(body: RefreshTokenDtoType): RefreshTokenParams {
    return {
      refreshToken: cleanQueryString(body.refresh_token),
    };
  }

  public static toSwitchGroupInput(
    body: SwitchGroupDtoType,
    currentUser: AuthenticatedUserPayload,
  ): SwitchGroupParams {
    return {
      userId: currentUser.sub,
      groupUid: cleanQueryString(body.group_uid),
    };
  }

  public static toLogoutInput(
    body?: LogoutDtoType,
    currentUser?: JwtUserPayload | null,
  ): LogoutParams {
    return {
      refreshToken: cleanQueryString(body?.refresh_token),
      userId: currentUser?.sub,
    };
  }
}
