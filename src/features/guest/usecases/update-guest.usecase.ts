import {
  type Failure,
  ForbiddenFailure,
  NotFoundFailure,
  ValidationFailure,
} from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type { GuestEntity } from '@features/guest/domain/entities/guest.entity';
import type {
  GuestRepository,
  UpdateGuestParams,
} from '@features/guest/domain/repositories/guest.repository';

export interface UpdateGuestPayload {
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
}

export interface UpdateGuestInput {
  id?: string | null;
  currentUser: {
    email?: string;
    role?: string | null;
    group_uid?: string | null;
  };
  data: UpdateGuestPayload;
}

export class UpdateGuestUseCase implements UseCase<GuestEntity, UpdateGuestInput> {
  constructor(private readonly repository: GuestRepository) {}

  public async execute(input: UpdateGuestInput): Promise<Result<GuestEntity, Failure>> {
    const groupUid = input.currentUser.group_uid;
    if (!groupUid) {
      return Result.fail(new ValidationFailure('Pengguna belum memilih grup aktif.'));
    }

    const guestId = input.id;
    if (!guestId) {
      return Result.fail(new ValidationFailure('ID tamu wajib diisi.'));
    }

    const existingGuest = await this.repository.getGuestById(guestId, groupUid);
    if (!existingGuest) {
      return Result.fail(new NotFoundFailure('Data tamu tidak ditemukan.'));
    }

    if (input.currentUser.role === 'GUARD') {
      const isOwner = Boolean(
        existingGuest.guestCreatorEmail &&
          existingGuest.guestCreatorEmail === input.currentUser.email,
      );

      if (!isOwner) {
        return Result.fail(
          new ForbiddenFailure('Petugas (Guard) hanya dapat mengubah data tamu miliknya sendiri.'),
        );
      }
    }

    let canonicalGuestType: string | undefined;
    if (input.data.guestType !== undefined && input.data.guestType !== null) {
      const rawGuestType = input.data.guestType;
      if (!rawGuestType) {
        return Result.fail(
          new ValidationFailure(
            'Tipe tamu tidak boleh kosong dan harus terdaftar pada grup aktif.',
          ),
        );
      }
      const resolved = await this.repository.resolveCanonicalGuestType(rawGuestType, groupUid);
      if (!resolved) {
        return Result.fail(
          new ValidationFailure('Tipe tamu tidak valid atau tidak terdaftar pada grup aktif.'),
        );
      }
      canonicalGuestType = resolved;
    }

    if (input.data.guestToUserEmail) {
      const isHostValid = await this.repository.isValidHost(input.data.guestToUserEmail, groupUid);
      if (!isHostValid) {
        return Result.fail(
          new ValidationFailure('Penghuni yang dituju tidak terdaftar pada grup aktif.'),
        );
      }
    }

    const updateParams: UpdateGuestParams = {
      context: {
        id: existingGuest.id,
        groupUid,
      },
      data: {
        ...(input.data.guestName !== undefined ? { guestName: input.data.guestName } : {}),
        ...(input.data.guestDestination !== undefined
          ? { guestDestination: input.data.guestDestination }
          : {}),
        ...(canonicalGuestType !== undefined ? { guestType: canonicalGuestType } : {}),
        ...(input.data.guestPurpose !== undefined ? { guestPurpose: input.data.guestPurpose } : {}),
        ...(input.data.guestNumber !== undefined ? { guestNumber: input.data.guestNumber } : {}),
        ...(input.data.guestIdentity !== undefined
          ? { guestIdentity: input.data.guestIdentity }
          : {}),
        ...(input.data.guestIdentityType !== undefined
          ? { guestIdentityType: input.data.guestIdentityType }
          : {}),
        ...(input.data.guestAddress !== undefined ? { guestAddress: input.data.guestAddress } : {}),
        ...(input.data.guestToUserEmail !== undefined
          ? { guestToUserEmail: input.data.guestToUserEmail }
          : {}),
        ...(input.data.guestPhoto !== undefined ? { guestPhoto: input.data.guestPhoto } : {}),
      },
    };

    const updatedGuest = await this.repository.updateGuest(updateParams);

    return Result.ok(updatedGuest);
  }
}
