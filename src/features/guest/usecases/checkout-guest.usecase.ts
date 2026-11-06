import { type Failure, NotFoundFailure, ValidationFailure } from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type { GuestEntity } from '@features/guest/domain/entities/guest.entity';
import type { GuestRepository } from '@features/guest/domain/repositories/guest.repository';

export interface CheckoutGuestPayload {
  qrUid?: string | null;
}

export interface CheckoutGuestInput {
  id?: string | null;
  currentUser: {
    email?: string;
    group_uid?: string | null;
  };
  data?: CheckoutGuestPayload;
}

export class CheckoutGuestUseCase implements UseCase<GuestEntity, CheckoutGuestInput> {
  constructor(private readonly repository: GuestRepository) {}

  public async execute(input: CheckoutGuestInput): Promise<Result<GuestEntity, Failure>> {
    const groupUid = input.currentUser.group_uid;
    if (!groupUid) {
      return Result.fail(new ValidationFailure('Pengguna belum memilih grup aktif.'));
    }

    const identifier = input.data?.qrUid || input.id;
    if (!identifier) {
      return Result.fail(new ValidationFailure('ID atau QR tamu wajib diisi.'));
    }

    const existingGuest = await this.repository.getGuestById(identifier, groupUid);
    if (!existingGuest) {
      return Result.fail(new NotFoundFailure('Data tamu tidak ditemukan.'));
    }

    if (existingGuest.guestOutDt || existingGuest.guestCheckOutByEmail) {
      return Result.fail(new ValidationFailure('Tamu sudah keluar/selesai kunjungan sebelumnya.'));
    }

    const outDt = new Date();
    const updatedGuest = await this.repository.updateGuest({
      context: {
        id: existingGuest.id,
        groupUid,
        checkOutByEmail: input.currentUser.email,
      },
      system: {
        guestOutDt: outDt,
      },
    });

    return Result.ok(updatedGuest);
  }
}
