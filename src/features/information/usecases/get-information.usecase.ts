import type { Failure } from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type { InformationEntity } from '@features/information/domain/entities/information.entity';
import type { InformationRepository } from '@features/information/domain/repositories/information.repository';

export interface GetInformationInput {
  currentUser: {
    group_uid?: string | null;
  };
  search?: string;
  limit?: number;
  offset?: number;
  startDate?: string;
  endDate?: string;
  createdBy?: string;
}

export class GetInformationUseCase implements UseCase<InformationEntity[], GetInformationInput> {
  constructor(private readonly repository: InformationRepository) {}

  public async execute(input: GetInformationInput): Promise<Result<InformationEntity[], Failure>> {
    if (!input.currentUser.group_uid) {
      return Result.ok([]);
    }

    const items = await this.repository.getInformation(input.currentUser.group_uid, {
      search: input.search,
      limit: input.limit,
      offset: input.offset,
      startDate: input.startDate,
      endDate: input.endDate,
      createdBy: input.createdBy,
    });
    return Result.ok(items);
  }
}
