import { InformationPrismaDataSource } from '@features/information/data/datasources/information-prisma.datasource';
import type { InformationEntity } from '@features/information/domain/entities/information.entity';
import type {
  CreateInformationParams,
  FeedCreatorContext,
  InformationFilterParams,
  InformationRepository,
  UpdateInformationParams,
} from '@features/information/domain/repositories/information.repository';

export class InformationRepositoryImpl implements InformationRepository {
  constructor(
    private readonly prismaSource: InformationPrismaDataSource = new InformationPrismaDataSource(),
  ) {}

  public async getInformation(
    groupUid: string,
    filter?: InformationFilterParams,
  ): Promise<InformationEntity[]> {
    return this.prismaSource.getInformation(groupUid, filter);
  }

  public async getInformationById(id: string, groupUid: string): Promise<InformationEntity | null> {
    return this.prismaSource.getInformationById(id, groupUid);
  }

  public async getInformationByIds(ids: string[], groupUid: string): Promise<InformationEntity[]> {
    return this.prismaSource.getInformationByIds(ids, groupUid);
  }

  public async getFeedCreatorContext(
    groupUid: string,
    userEmail: string,
  ): Promise<FeedCreatorContext> {
    return this.prismaSource.getFeedCreatorContext(groupUid, userEmail);
  }

  public async createInformation(params: CreateInformationParams): Promise<InformationEntity> {
    return this.prismaSource.createInformation(params);
  }

  public async updateInformation(params: UpdateInformationParams): Promise<InformationEntity> {
    return this.prismaSource.updateInformation(params);
  }

  public async deleteManyInformation(ids: string[], groupUid: string): Promise<number> {
    return this.prismaSource.deleteManyInformation(ids, groupUid);
  }
}
