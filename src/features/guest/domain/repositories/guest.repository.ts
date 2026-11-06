import type { GuestEntity } from '@features/guest/domain/entities/guest.entity';
import type { MasterGuestTypeItem } from '@features/guest/domain/entities/master-data.entity';

export interface GuestFilterParams {
  search?: string;
  limit?: number;
  offset?: number;
  startDate?: string;
  endDate?: string;
  createdBy?: string;
  isActive?: boolean;
}

export interface CreateGuestSystem {
  guestUid: string;
  qrUid: string;
  guestCategory: string;
  guestDt: Date;
}

export interface CreateGuestContext {
  groupUid: string;
  creatorEmail: string;
  creatorName: string;
}

export type CreateGuestUserContext = CreateGuestContext;

export interface CreateGuestData {
  guestName: string;
  guestDestination: string;
  guestType: string;
  guestPurpose?: string | null;
  guestNumber?: string | null;
  guestIdentity?: string | null;
  guestIdentityType?: string | null;
  guestAddress?: string | null;
  guestToUserEmail?: string | null;
  guestPhoto?: string | null;
}

export interface CreateGuestParams {
  system: CreateGuestSystem;
  context: CreateGuestContext;
  data: CreateGuestData;
}

export interface UpdateGuestContext {
  id: string;
  groupUid: string;
  checkOutByEmail?: string | null;
}

export type UpdateGuestUserContext = UpdateGuestContext;

export interface UpdateGuestSystem {
  guestAckDt?: Date | null;
  guestOutDt?: Date | null;
}

export interface UpdateGuestData {
  guestName?: string | null;
  guestDestination?: string | null;
  guestPurpose?: string | null;
  guestNumber?: string | null;
  guestIdentity?: string | null;
  guestIdentityType?: string | null;
  guestCategory?: string | null;
  guestType?: string | null;
  guestAddress?: string | null;
  guestToUserEmail?: string | null;
  guestPhoto?: string | null;
}

export interface UpdateGuestParams {
  context: UpdateGuestContext;
  data?: UpdateGuestData;
  system?: UpdateGuestSystem;
}

export interface GuestRepository {
  getGuests(groupUid: string, filter?: GuestFilterParams): Promise<GuestEntity[]>;
  getGuestById(id: string, groupUid: string): Promise<GuestEntity | null>;
  getGuestsByIds(ids: string[], groupUid: string): Promise<GuestEntity[]>;
  createGuest(params: CreateGuestParams): Promise<GuestEntity>;
  updateGuest(params: UpdateGuestParams): Promise<GuestEntity>;
  deleteManyGuests(ids: string[], groupUid: string): Promise<number>;
  isValidGuestType(guestType: string, groupUid: string): Promise<boolean>;
  resolveCanonicalGuestType(guestType: string, groupUid: string): Promise<string | null>;
  isValidHost(email: string, groupUid: string): Promise<boolean>;
  getMasterGuestTypes(groupUid: string): Promise<MasterGuestTypeItem[]>;
}
