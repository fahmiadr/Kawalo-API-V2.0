import {
  type Failure,
  ForbiddenFailure,
  NotFoundFailure,
  ValidationFailure,
} from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type {
  IncidentEventPublisher,
  IncidentRepository,
} from '@features/incident/domain/repositories/incident.repository';
import { safePublish } from '@features/incident/usecases/safe-publish';

export interface DeleteIncidentInput {
  id: string;
  currentUser: { allow_admin?: boolean; group_uid?: string | null };
}

export class DeleteIncidentUseCase implements UseCase<null, DeleteIncidentInput> {
  constructor(
    private readonly repository: IncidentRepository,
    private readonly publisher: IncidentEventPublisher,
  ) {}

  public async execute({ id, currentUser }: DeleteIncidentInput): Promise<Result<null, Failure>> {
    const groupUid = currentUser.group_uid;
    if (!groupUid) return Result.fail(new ValidationFailure('Pengguna belum memilih grup aktif.'));
    if (!currentUser.allow_admin) {
      return Result.fail(new ForbiddenFailure('Hanya administrator yang dapat menghapus insiden.'));
    }

    const incident = await this.repository.findById(id, groupUid);
    if (!incident) return Result.fail(new NotFoundFailure('Insiden tidak ditemukan.'));

    const deletedAt = await this.repository.softDelete(incident.id);
    await safePublish(this.publisher, incident.id, {
      type: 'incident_deleted',
      incidentId: incident.id,
      deletedAt,
    });
    return Result.ok(null);
  }
}
