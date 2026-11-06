import { type Failure, ValidationFailure } from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import { generatePrefixedUuidV7 } from '@core/utils/uuid.util';
import type { InformationEntity } from '@features/information/domain/entities/information.entity';
import type { InformationRepository } from '@features/information/domain/repositories/information.repository';

export const ALLOWED_FEED_TYPES = ['GENERAL', 'JOIN', 'INCIDENT', 'EVENT', 'OTHER'] as const;

export type AllowedFeedType = (typeof ALLOWED_FEED_TYPES)[number];

export interface CreateInformationPayload {
  title?: string | null;
  text?: string;
  feedType?: string;
  feedFile?: string;
  photos?: string[];
  eventLocation?: string;
  glat?: string;
  glong?: string;
}

export interface CreateInformationInput {
  currentUser: {
    email?: string;
    name?: string;
    role?: string | null;
    group_uid?: string | null;
  };
  data: CreateInformationPayload;
}

export class CreateInformationUseCase
  implements UseCase<InformationEntity, CreateInformationInput>
{
  constructor(private readonly repository: InformationRepository) {}

  public async execute(input: CreateInformationInput): Promise<Result<InformationEntity, Failure>> {
    const { email, name, group_uid: groupUid } = input.currentUser;
    if (!groupUid) {
      return Result.fail(new ValidationFailure('Pengguna belum memilih grup aktif.'));
    }

    if (!email || !name) {
      return Result.fail(new ValidationFailure('Data personil (email dan nama) tidak valid.'));
    }

    if (!input.data.title) {
      return Result.fail(new ValidationFailure('Judul informasi wajib diisi.'));
    }

    let feedType: AllowedFeedType = 'OTHER';
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

    const creatorContext = await this.repository.getFeedCreatorContext(groupUid, email);
    const feedUid = generatePrefixedUuidV7('feed');
    const feedDt = new Date();

    const photos = input.data.photos;
    const feedPic = photos?.[0] ?? null;
    const feedPic2 = photos?.[1] ?? null;
    const feedPic3 = photos?.[2] ?? null;
    const feedPic4 = photos?.[3] ?? null;

    const created = await this.repository.createInformation({
      system: {
        feedUid,
        feedDt,
        feedGroupName: creatorContext.groupName,
        feedByProfilePic: creatorContext.userProfilePic,
      },
      context: {
        groupUid,
        creatorEmail: email,
        creatorName: name,
      },
      data: {
        title: input.data.title,
        text: input.data.text,
        feedType,
        feedFile: input.data.feedFile,
        feedPic,
        feedPic2,
        feedPic3,
        feedPic4,
        eventLocation: input.data.eventLocation,
        glat: input.data.glat,
        glong: input.data.glong,
      },
    });

    return Result.ok(created);
  }
}
