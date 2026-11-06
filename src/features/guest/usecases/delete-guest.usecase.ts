import {
  type Failure,
  ForbiddenFailure,
  NotFoundFailure,
  ValidationFailure,
} from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type { GuestRepository } from '@features/guest/domain/repositories/guest.repository';

export interface DeleteGuestInput {
  id?: string | null;
  ids?: string[] | null;
  currentUser: {
    email?: string;
    role?: string | null;
    group_uid?: string | null;
  };
}

export interface DeleteGuestOutput {
  deletedCount: number;
}

export class DeleteGuestUseCase implements UseCase<DeleteGuestOutput, DeleteGuestInput> {
  constructor(private readonly repository: GuestRepository) {}

  public async execute(input: DeleteGuestInput): Promise<Result<DeleteGuestOutput, Failure>> {
    const groupUid = input.currentUser.group_uid;
    if (!groupUid) {
      return Result.fail(new ValidationFailure('Pengguna belum memilih grup aktif.'));
    }

    const rawIds = [...(input.ids ?? []), ...(input.id ? [input.id] : [])];
    const targetIds = Array.from(new Set(rawIds.filter((id): id is string => Boolean(id))));

    if (targetIds.length === 0) {
      return Result.fail(new ValidationFailure('ID tamu wajib diisi.'));
    }

    const existingGuests = await this.repository.getGuestsByIds(targetIds, groupUid);
    if (existingGuests.length === 0) {
      return Result.fail(new NotFoundFailure('Data tamu tidak ditemukan.'));
    }

    if (existingGuests.length !== targetIds.length) {
      return Result.fail(
        new NotFoundFailure('Satu atau lebih data tamu tidak ditemukan dalam grup aktif.'),
      );
    }

    if (input.currentUser.role === 'GUARD') {
      const unownedGuest = existingGuests.find(
        (guest) => !guest.guestCreatorEmail || guest.guestCreatorEmail !== input.currentUser.email,
      );

      if (unownedGuest) {
        return Result.fail(
          new ForbiddenFailure('Petugas (Guard) hanya dapat menghapus data tamu miliknya sendiri.'),
        );
      }
    }

    const deletedCount = await this.repository.deleteManyGuests(targetIds, groupUid);
    return Result.ok({ deletedCount });
  }
}
