import type { Failure } from '@core/error/failure';
import type { Result } from '@core/error/result';

export interface UseCase<Type, Params> {
  execute(params: Params): Promise<Result<Type, Failure>>;
}

export class NoParams {}
