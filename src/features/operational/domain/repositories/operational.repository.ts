import type { OperationalEntity } from '@features/operational/domain/entities/operational.entity';

export interface OperationalFilterParams {
  search?: string;
  limit?: number;
  offset?: number;
  startDate?: string;
  endDate?: string;
  groupUid?: string | null;
}

export interface OperationalRepository {
  getOperationals(filter?: OperationalFilterParams): Promise<OperationalEntity[]>;
}
