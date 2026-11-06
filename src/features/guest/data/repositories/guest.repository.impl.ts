import { GuestPrismaDataSource } from '@features/guest/data/datasources/guest-prisma.datasource';
import type { GuestEntity } from '@features/guest/domain/entities/guest.entity';
import type { MasterGuestTypeItem } from '@features/guest/domain/entities/master-data.entity';
import type {
  CreateGuestParams,
  GuestFilterParams,
  GuestRepository,
  UpdateGuestParams,
} from '@features/guest/domain/repositories/guest.repository';

export class GuestRepositoryImpl implements GuestRepository {
  constructor(private readonly prismaSource: GuestPrismaDataSource = new GuestPrismaDataSource()) {}

  public async getGuests(groupUid: string, filter?: GuestFilterParams): Promise<GuestEntity[]> {
    return this.prismaSource.getGuests(groupUid, filter);
  }

  public async getGuestById(id: string, groupUid: string): Promise<GuestEntity | null> {
    return this.prismaSource.getGuestById(id, groupUid);
  }

  public async getGuestsByIds(ids: string[], groupUid: string): Promise<GuestEntity[]> {
    return this.prismaSource.getGuestsByIds(ids, groupUid);
  }

  public async createGuest(params: CreateGuestParams): Promise<GuestEntity> {
    return this.prismaSource.createGuest(params);
  }

  public async updateGuest(params: UpdateGuestParams): Promise<GuestEntity> {
    return this.prismaSource.updateGuest(params);
  }

  public async deleteManyGuests(ids: string[], groupUid: string): Promise<number> {
    return this.prismaSource.deleteManyGuests(ids, groupUid);
  }

  public async isValidGuestType(guestType: string, groupUid: string): Promise<boolean> {
    if (!guestType || !groupUid) {
      return false;
    }
    const canonical = await this.resolveCanonicalGuestType(guestType, groupUid);
    return Boolean(canonical);
  }

  public async resolveCanonicalGuestType(
    guestType: string,
    groupUid: string,
  ): Promise<string | null> {
    if (!guestType || !groupUid) {
      return null;
    }
    return this.prismaSource.resolveCanonicalGuestType(guestType, groupUid);
  }

  public async isValidHost(email: string, groupUid: string): Promise<boolean> {
    if (!email || !groupUid) {
      return false;
    }
    return this.prismaSource.isGroupMemberJoined(email, groupUid);
  }

  public async getMasterGuestTypes(groupUid: string): Promise<MasterGuestTypeItem[]> {
    if (!groupUid) {
      return [];
    }
    return this.prismaSource.getMasterGuestTypes(groupUid);
  }
}
