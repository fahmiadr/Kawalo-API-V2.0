import { type Failure, NotFoundFailure, ValidationFailure } from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import { generatePrefixedUuidV7 } from '@core/utils/uuid.util';
import type { IncidentReplyEntity } from '@features/incident/domain/entities/incident.entity';
import type {
  IncidentEventPublisher,
  IncidentRepository,
} from '@features/incident/domain/repositories/incident.repository';
import { safePublish } from '@features/incident/usecases/safe-publish';

export interface PostIncidentReplyInput {
  id: string;
  currentUser: { email: string; name?: string; role?: string | null; group_uid?: string | null };
  data: { message: string; photos: string[] };
}

export class PostIncidentReplyUseCase
  implements UseCase<IncidentReplyEntity, PostIncidentReplyInput>
{
  constructor(
    private readonly repository: IncidentRepository,
    private readonly publisher: IncidentEventPublisher,
  ) {}

  public async execute({
    id,
    currentUser,
    data,
  }: PostIncidentReplyInput): Promise<Result<IncidentReplyEntity, Failure>> {
    const groupUid = currentUser.group_uid;
    if (!groupUid) return Result.fail(new ValidationFailure('Pengguna belum memilih grup aktif.'));
    if (!data.message && data.photos.length === 0) {
      return Result.fail(new ValidationFailure('Pesan atau foto wajib diisi.'));
    }

    const incident = await this.repository.findById(id, groupUid);
    if (!incident) return Result.fail(new NotFoundFailure('Insiden tidak ditemukan.'));
    if (incident.status === 'RESOLVED') {
      return Result.fail(new ValidationFailure('Insiden sudah diselesaikan, diskusi ditutup.'));
    }

    const reply = await this.repository.addReply({
      id: generatePrefixedUuidV7('rpl'),
      incidentId: incident.id,
      senderEmail: currentUser.email,
      senderName: currentUser.name ?? currentUser.email,
      senderRole: currentUser.role ?? 'USER',
      message: data.message,
      photos: data.photos,
    });
    await safePublish(this.publisher, incident.id, { type: 'new_reply', reply });
    return Result.ok(reply);
  }
}
