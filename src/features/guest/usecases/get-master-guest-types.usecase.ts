import type { Failure } from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type { MasterGuestTypeItem } from '@features/guest/domain/entities/master-data.entity';
import type { GuestRepository } from '@features/guest/domain/repositories/guest.repository';

export interface GetMasterGuestTypesInput {
  currentUser: {
    group_uid?: string | null;
  };
}

export class GetMasterGuestTypesUseCase
  implements UseCase<MasterGuestTypeItem[], GetMasterGuestTypesInput>
{
  constructor(private readonly repository: GuestRepository) {}

  public async execute(
    input: GetMasterGuestTypesInput,
  ): Promise<Result<MasterGuestTypeItem[], Failure>> {
    if (!input.currentUser.group_uid) {
      return Result.ok([]);
    }

    const items = await this.repository.getMasterGuestTypes(input.currentUser.group_uid);
    return Result.ok(items);
  }
}
