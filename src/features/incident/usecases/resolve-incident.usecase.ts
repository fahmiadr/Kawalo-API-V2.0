import { type Failure, NotFoundFailure, ValidationFailure } from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type { IncidentEntity } from '@features/incident/domain/entities/incident.entity';
import type {
  IncidentEventPublisher,
  IncidentRepository,
} from '@features/incident/domain/repositories/incident.repository';
import { safePublish } from '@features/incident/usecases/safe-publish';

export interface ResolveIncidentInput {
  id: string;
  currentUser: { email: string; name?: string; group_uid?: string | null };
}

export class ResolveIncidentUseCase implements UseCase<IncidentEntity, ResolveIncidentInput> {
  constructor(
    private readonly repository: IncidentRepository,
    private readonly publisher: IncidentEventPublisher,
  ) {}

  public async execute({
    id,
    currentUser,
  }: ResolveIncidentInput): Promise<Result<IncidentEntity, Failure>> {
    const groupUid = currentUser.group_uid;
    if (!groupUid) return Result.fail(new ValidationFailure('Pengguna belum memilih grup aktif.'));

    const incident = await this.repository.findById(id, groupUid);
    if (!incident) return Result.fail(new NotFoundFailure('Insiden tidak ditemukan.'));
    if (incident.status === 'RESOLVED') return Result.ok(incident); // idempoten, tanpa event ganda

    const resolvedByName = currentUser.name ?? currentUser.email;
    const resolved = await this.repository.resolve(incident.id, {
      email: currentUser.email,
      name: resolvedByName,
    });
    if (!resolved) return Result.fail(new NotFoundFailure('Insiden tidak ditemukan.'));

    // Bila dua orang menyelesaikan bersamaan, hanya pemenang (resolvedBy == kita) yang publish.
    if (resolved.resolvedBy === currentUser.email && resolved.resolvedAt) {
      await safePublish(this.publisher, incident.id, {
        type: 'thread_resolved',
        incidentId: incident.id,
        resolvedBy: currentUser.email,
        resolvedByName,
        resolvedAt: resolved.resolvedAt,
      });
    }
    return Result.ok(resolved);
  }
}
