import {
  type Failure,
  ForbiddenFailure,
  NotFoundFailure,
  ValidationFailure,
} from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type { InformationEntity } from '@features/information/domain/entities/information.entity';
import type {
  InformationRepository,
  UpdateInformationParams,
} from '@features/information/domain/repositories/information.repository';
import {
  ALLOWED_FEED_TYPES,
  type AllowedFeedType,
} from '@features/information/usecases/create-information.usecase';

export interface UpdateInformationPayload {
  title?: string | null;
  text?: string | null;
  feedType?: string | null;
  feedFile?: string | null;
  photos?: string[];
  eventLocation?: string | null;
  glat?: string | null;
  glong?: string | null;
}

export interface UpdateInformationInput {
  id?: string | null;
  currentUser: {
    email?: string;
    role?: string | null;
    group_uid?: string | null;
  };
  data: UpdateInformationPayload;
}

export class UpdateInformationUseCase
  implements UseCase<InformationEntity, UpdateInformationInput>
{
  constructor(private readonly repository: InformationRepository) {}

  public async execute(input: UpdateInformationInput): Promise<Result<InformationEntity, Failure>> {
    const groupUid = input.currentUser.group_uid;
    if (!groupUid) {
      return Result.fail(new ValidationFailure('Pengguna belum memilih grup aktif.'));
    }

    const informationId = input.id;
    if (!informationId) {
      return Result.fail(new ValidationFailure('ID informasi wajib diisi.'));
    }

    const existing = await this.repository.getInformationById(informationId, groupUid);
    if (!existing) {
      return Result.fail(new NotFoundFailure('Data informasi tidak ditemukan.'));
    }

    if (input.currentUser.role === 'GUARD') {
      if (!existing.userEmail || existing.userEmail !== input.currentUser.email) {
        return Result.fail(
          new ForbiddenFailure('Petugas (Guard) hanya dapat mengubah informasi miliknya sendiri.'),
        );
      }
    }

    let feedType: AllowedFeedType | undefined;
    if (input.data.feedType) {
      const normalizedType = input.data.feedType.toUpperCase() as AllowedFeedType;
      if (!ALLOWED_FEED_TYPES.includes(normalizedType)) {
        return Result.fail(
          new ValidationFailure(
            `Tipe informasi (feed_type) harus salah satu dari: ${ALLOWED_FEED_TYPES.join(', ')}.`,
          ),
        );
      }
      feedType = normalizedType;
    }

    let photoFields: {
      feedPic?: string | null;
      feedPic2?: string | null;
      feedPic3?: string | null;
      feedPic4?: string | null;
    } = {};

    if (input.data.photos !== undefined) {
      const photos = input.data.photos;
      photoFields = {
        feedPic: photos?.[0] ?? null,
        feedPic2: photos?.[1] ?? null,
        feedPic3: photos?.[2] ?? null,
        feedPic4: photos?.[3] ?? null,
      };
    }

    const updateParams: UpdateInformationParams = {
      context: {
        id: informationId,
        groupUid,
      },
      data: {
        ...(input.data.title !== undefined ? { title: input.data.title } : {}),
        ...(input.data.text !== undefined ? { text: input.data.text } : {}),
        ...(feedType !== undefined ? { feedType } : {}),
        ...(input.data.feedFile !== undefined ? { feedFile: input.data.feedFile } : {}),
        ...photoFields,
        ...(input.data.eventLocation !== undefined
          ? { eventLocation: input.data.eventLocation }
          : {}),
        ...(input.data.glat !== undefined ? { glat: input.data.glat } : {}),
        ...(input.data.glong !== undefined ? { glong: input.data.glong } : {}),
      },
    };

    const updated = await this.repository.updateInformation(updateParams);
    return Result.ok(updated);
  }
}
