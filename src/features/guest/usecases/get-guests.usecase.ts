import type { Failure } from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type { GuestEntity } from '@features/guest/domain/entities/guest.entity';
import type { GuestRepository } from '@features/guest/domain/repositories/guest.repository';

export interface GetGuestsInput {
  currentUser: {
    group_uid?: string | null;
  };
  search?: string;
  limit?: number;
  offset?: number;
  startDate?: string;
  endDate?: string;
  createdBy?: string;
  isActive?: boolean;
}

export class GetGuestsUseCase implements UseCase<GuestEntity[], GetGuestsInput> {
  constructor(private readonly repository: GuestRepository) {}

  public async execute(input: GetGuestsInput): Promise<Result<GuestEntity[], Failure>> {
    if (!input.currentUser.group_uid) {
      return Result.ok([]);
    }

    const guests = await this.repository.getGuests(input.currentUser.group_uid, {
      search: input.search,
      limit: input.limit,
      offset: input.offset,
      startDate: input.startDate,
      endDate: input.endDate,
      createdBy: input.createdBy,
      isActive: input.isActive,
    });
    return Result.ok(guests);
  }
}
