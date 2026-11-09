import { type Failure, NotFoundFailure, ValidationFailure } from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type {
  IncidentEntity,
  IncidentReplyEntity,
} from '@features/incident/domain/entities/incident.entity';
import type { IncidentRepository } from '@features/incident/domain/repositories/incident.repository';

export interface GetIncidentDetailInput {
  id: string;
  currentUser: { group_uid?: string | null };
}

export interface IncidentDetail {
  incident: IncidentEntity;
  replies: IncidentReplyEntity[];
}

export class GetIncidentDetailUseCase implements UseCase<IncidentDetail, GetIncidentDetailInput> {
  constructor(private readonly repository: IncidentRepository) {}

  public async execute(input: GetIncidentDetailInput): Promise<Result<IncidentDetail, Failure>> {
    const groupUid = input.currentUser.group_uid;
    if (!groupUid) return Result.fail(new ValidationFailure('Pengguna belum memilih grup aktif.'));

    const incident = await this.repository.findById(input.id, groupUid);
    if (!incident) return Result.fail(new NotFoundFailure('Insiden tidak ditemukan.'));

    return Result.ok({ incident, replies: await this.repository.listReplies(incident.id) });
  }
}
