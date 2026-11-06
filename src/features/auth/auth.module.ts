import { AuthRepositoryImpl } from '@features/auth/data/repositories/auth.repository.impl';
import { AuthHandler } from '@features/auth/presentation/handlers/auth.handler';
import { LoginUseCase } from '@features/auth/usecases/login.usecase';
import { LogoutUseCase } from '@features/auth/usecases/logout.usecase';
import { RefreshTokenUseCase } from '@features/auth/usecases/refresh-token.usecase';
import { SwitchGroupUseCase } from '@features/auth/usecases/switch-group.usecase';

const authRepository = new AuthRepositoryImpl();
const loginUseCase = new LoginUseCase(authRepository);
const refreshTokenUseCase = new RefreshTokenUseCase(authRepository);
const switchGroupUseCase = new SwitchGroupUseCase(authRepository);
const logoutUseCase = new LogoutUseCase(authRepository);

export const authHandler = new AuthHandler(
  loginUseCase,
  refreshTokenUseCase,
  switchGroupUseCase,
  logoutUseCase,
);
