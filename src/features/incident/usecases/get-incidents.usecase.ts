import type { Failure } from '@core/error/failure';
import { Result } from '@core/error/result';
import type { UseCase } from '@core/usecases/usecase.base';
import type { IncidentEntity } from '@features/incident/domain/entities/incident.entity';
import type {
  IncidentFilterParams,
  IncidentRepository,
} from '@features/incident/domain/repositories/incident.repository';

export interface GetIncidentsInput extends IncidentFilterParams {
  currentUser: { group_uid?: string | null };
}

export class GetIncidentsUseCase implements UseCase<IncidentEntity[], GetIncidentsInput> {
  constructor(private readonly repository: IncidentRepository) {}

  public async execute({
    currentUser,
    ...filter
  }: GetIncidentsInput): Promise<Result<IncidentEntity[], Failure>> {
    if (!currentUser.group_uid) return Result.ok([]);
    return Result.ok(await this.repository.list(currentUser.group_uid, filter));
  }
}
