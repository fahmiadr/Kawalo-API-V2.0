import {
  type Failure,
  ForbiddenFailure,
  NotFoundFailure,
  ValidationFailure,
} from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type { GuestEntity } from '@features/guest/domain/entities/guest.entity';
import type { GuestRepository } from '@features/guest/domain/repositories/guest.repository';

export interface AcknowledgeGuestPayload {
  qrUid?: string | null;
}

export interface AcknowledgeGuestInput {
  id?: string | null;
  currentUser: {
    email?: string;
    role?: string | null;
    group_uid?: string | null;
  };
  data?: AcknowledgeGuestPayload;
}

export class AcknowledgeGuestUseCase implements UseCase<GuestEntity, AcknowledgeGuestInput> {
  constructor(private readonly repository: GuestRepository) {}

  public async execute(input: AcknowledgeGuestInput): Promise<Result<GuestEntity, Failure>> {
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

    // Cek apakah tamu sudah checkout
    if (existingGuest.guestOutDt || existingGuest.guestCheckOutByEmail) {
      return Result.fail(
        new ValidationFailure(
          'Tamu sudah keluar/selesai kunjungan, tidak dapat dikonfirmasi lagi.',
        ),
      );
    }

    // Cek apakah kunjungan sudah dikonfirmasi sebelumnya (tidak bisa ack dua kali)
    if (existingGuest.guestAckDt) {
      return Result.fail(new ValidationFailure('Kunjungan tamu sudah dikonfirmasi sebelumnya.'));
    }

    // Role check: ADMIN, GUARD, atau penghuni yang dituju (host)
    const role = input.currentUser.role;
    const isStaff = role === 'ADMIN' || role === 'GUARD';
    const isHost = Boolean(
      existingGuest.guestToUserEmail && existingGuest.guestToUserEmail === input.currentUser.email,
    );

    if (!isStaff && !isHost) {
      return Result.fail(
        new ForbiddenFailure(
          'Hanya Admin, Petugas (Guard), atau Penghuni yang dituju yang dapat mengonfirmasi tamu.',
        ),
      );
    }

    const ackDt = new Date();
    const updatedGuest = await this.repository.updateGuest({
      context: {
        id: existingGuest.id,
        groupUid,
      },
      system: {
        guestAckDt: ackDt,
      },
    });

    return Result.ok(updatedGuest);
  }
}
