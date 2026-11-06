import type { Failure } from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type { OperationalEntity } from '@features/operational/domain/entities/operational.entity';
import type { OperationalRepository } from '@features/operational/domain/repositories/operational.repository';

export interface GetOperationalInput {
  currentUser: { group_uid?: string | null };
  search?: string;
  limit?: number;
  offset?: number;
  startDate?: string;
  endDate?: string;
}

export class GetOperationalDataUseCase
  implements UseCase<OperationalEntity[], GetOperationalInput>
{
  constructor(private readonly repository: OperationalRepository) {}

  public async execute(input: GetOperationalInput): Promise<Result<OperationalEntity[], Failure>> {
    const groupUid = input.currentUser.group_uid;
    if (!groupUid) {
      return Result.ok([]);
    }

    const operationals = await this.repository.getOperationals({
      search: input.search,
      limit: input.limit,
      offset: input.offset,
      startDate: input.startDate,
      endDate: input.endDate,
      groupUid,
    });
    return Result.ok(operationals);
  }
}
