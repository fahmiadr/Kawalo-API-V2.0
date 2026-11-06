import { type Failure, ValidationFailure } from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import { generatePrefixedUuidV7, generateUuidV4 } from '@core/utils/uuid.util';
import type { GuestEntity } from '@features/guest/domain/entities/guest.entity';
import type { GuestRepository } from '@features/guest/domain/repositories/guest.repository';

export interface CreateGuestPayload {
  guestName?: string | null;
  guestDestination?: string | null;
  guestPurpose?: string | null;
  guestNumber?: string | null;
  guestIdentity?: string | null;
  guestIdentityType?: string | null;
  guestType?: string | null;
  guestAddress?: string | null;
  guestToUserEmail?: string | null;
  guestPhoto?: string | null;
  guestDt?: string | null;
}

export interface CreateGuestInput {
  currentUser: {
    email?: string;
    name?: string;
    group_uid?: string | null;
  };
  data: CreateGuestPayload;
}

export class CreateGuestUseCase implements UseCase<GuestEntity, CreateGuestInput> {
  constructor(private readonly repository: GuestRepository) {}

  public async execute(input: CreateGuestInput): Promise<Result<GuestEntity, Failure>> {
    const { email, name, group_uid: groupUid } = input.currentUser;

    if (!groupUid) {
      return Result.fail(new ValidationFailure('Pengguna belum memilih grup aktif.'));
    }

    if (!email || !name) {
      return Result.fail(new ValidationFailure('Data personil (email dan nama) tidak valid.'));
    }

    if (!input.data.guestName) {
      return Result.fail(new ValidationFailure('Nama tamu wajib diisi.'));
    }

    if (!input.data.guestDestination) {
      return Result.fail(new ValidationFailure('Destinasi tamu wajib diisi.'));
    }

    if (!input.data.guestType) {
      return Result.fail(
        new ValidationFailure('Tipe tamu wajib diisi dan harus terdaftar pada grup aktif.'),
      );
    }

    const canonicalGuestType = await this.repository.resolveCanonicalGuestType(
      input.data.guestType,
      groupUid,
    );
    if (!canonicalGuestType) {
      return Result.fail(
        new ValidationFailure('Tipe tamu tidak valid atau tidak terdaftar pada grup aktif.'),
      );
    }

    if (input.data.guestToUserEmail) {
      const isHostValid = await this.repository.isValidHost(input.data.guestToUserEmail, groupUid);
      if (!isHostValid) {
        return Result.fail(
          new ValidationFailure('Penghuni yang dituju tidak terdaftar pada grup aktif.'),
        );
      }
    }

    const guestUid = generatePrefixedUuidV7('guest');
    const qrUid = generateUuidV4();
    const guestDt = input.data.guestDt ? new Date(input.data.guestDt) : new Date();

    const createdGuest = await this.repository.createGuest({
      system: {
        guestUid,
        qrUid,
        guestCategory: 'Umum',
        guestDt,
      },
      context: {
        groupUid,
        creatorEmail: email,
        creatorName: name,
      },
      data: {
        guestName: input.data.guestName,
        guestDestination: input.data.guestDestination,
        guestType: canonicalGuestType,
        guestPurpose: input.data.guestPurpose,
        guestNumber: input.data.guestNumber,
        guestIdentity: input.data.guestIdentity,
        guestIdentityType: input.data.guestIdentityType,
        guestAddress: input.data.guestAddress,
        guestToUserEmail: input.data.guestToUserEmail,
        guestPhoto: input.data.guestPhoto,
      },
    });

    return Result.ok(createdGuest);
  }
}
