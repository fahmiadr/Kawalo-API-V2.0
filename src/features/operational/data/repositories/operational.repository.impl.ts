import { OperationalPrismaDataSource } from '@features/operational/data/datasources/operational-prisma.datasource';
import type { OperationalEntity } from '@features/operational/domain/entities/operational.entity';
import type {
  OperationalFilterParams,
  OperationalRepository,
} from '@features/operational/domain/repositories/operational.repository';

export class OperationalRepositoryImpl implements OperationalRepository {
  constructor(
    private readonly prismaSource: OperationalPrismaDataSource = new OperationalPrismaDataSource(),
  ) {}

  public async getOperationals(filter?: OperationalFilterParams): Promise<OperationalEntity[]> {
    return this.prismaSource.getOperationals(filter);
  }
}
