import { type Failure, NotFoundFailure, ValidationFailure } from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type { GuestEntity } from '@features/guest/domain/entities/guest.entity';
import type { GuestRepository } from '@features/guest/domain/repositories/guest.repository';

export interface GetGuestByIdInput {
  id?: string | null;
  currentUser: {
    group_uid?: string | null;
  };
}

export class GetGuestByIdUseCase implements UseCase<GuestEntity | null, GetGuestByIdInput> {
  constructor(private readonly repository: GuestRepository) {}

  public async execute(input: GetGuestByIdInput): Promise<Result<GuestEntity | null, Failure>> {
    if (!input.currentUser.group_uid) {
      return Result.ok(null);
    }

    if (!input.id) {
      return Result.fail(new ValidationFailure('ID atau QR tamu wajib diisi.'));
    }

    const guest = await this.repository.getGuestById(input.id, input.currentUser.group_uid);
    if (!guest) {
      return Result.fail(new NotFoundFailure('Data tamu tidak ditemukan.'));
    }

    return Result.ok(guest);
  }
}
