import {
  type Failure,
  ForbiddenFailure,
  NotFoundFailure,
  ValidationFailure,
} from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type { InformationRepository } from '@features/information/domain/repositories/information.repository';

export interface DeleteInformationInput {
  id?: string | null;
  ids?: string[] | null;
  currentUser: {
    email?: string;
    role?: string | null;
    group_uid?: string | null;
  };
}

export interface DeleteInformationOutput {
  deletedCount: number;
}

export class DeleteInformationUseCase
  implements UseCase<DeleteInformationOutput, DeleteInformationInput>
{
  constructor(private readonly repository: InformationRepository) {}

  public async execute(
    input: DeleteInformationInput,
  ): Promise<Result<DeleteInformationOutput, Failure>> {
    const groupUid = input.currentUser.group_uid;
    if (!groupUid) {
      return Result.fail(new ValidationFailure('Pengguna belum memilih grup aktif.'));
    }

    const rawIds = [...(input.ids ?? []), ...(input.id ? [input.id] : [])];
    const targetIds = Array.from(new Set(rawIds.filter((id): id is string => Boolean(id))));

    if (targetIds.length === 0) {
      return Result.fail(new ValidationFailure('ID informasi wajib diisi.'));
    }

    const existingItems = await this.repository.getInformationByIds(targetIds, groupUid);
    if (existingItems.length === 0) {
      return Result.fail(new NotFoundFailure('Data informasi tidak ditemukan.'));
    }

    if (existingItems.length !== targetIds.length) {
      return Result.fail(
        new NotFoundFailure('Satu atau lebih data informasi tidak ditemukan dalam grup aktif.'),
      );
    }

    if (input.currentUser.role === 'GUARD') {
      const unownedItem = existingItems.find(
        (item) => !item.userEmail || item.userEmail !== input.currentUser.email,
      );

      if (unownedItem) {
        return Result.fail(
          new ForbiddenFailure('Petugas (Guard) hanya dapat menghapus informasi miliknya sendiri.'),
        );
      }
    }

    const deletedCount = await this.repository.deleteManyInformation(targetIds, groupUid);
    return Result.ok({ deletedCount });
  }
}
