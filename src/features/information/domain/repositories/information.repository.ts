import type { InformationEntity } from '@features/information/domain/entities/information.entity';

export interface InformationFilterParams {
  search?: string;
  limit?: number;
  offset?: number;
  startDate?: string;
  endDate?: string;
  createdBy?: string;
}

export interface FeedCreatorContext {
  groupName: string | null;
  userProfilePic: string | null;
}

export interface CreateInformationSystem {
  feedUid: string;
  feedDt: Date;
  feedGroupName?: string | null;
  feedByProfilePic?: string | null;
}

export interface CreateInformationContext {
  groupUid: string;
  creatorEmail: string;
  creatorName: string;
}

export type CreateInformationUserContext = CreateInformationContext;

export interface CreateInformationData {
  title: string;
  text?: string;
  feedType: string;
  feedFile?: string;
  feedPic?: string | null;
  feedPic2?: string | null;
  feedPic3?: string | null;
  feedPic4?: string | null;
  eventLocation?: string;
  glat?: string;
  glong?: string;
}

export interface CreateInformationParams {
  system: CreateInformationSystem;
  context: CreateInformationContext;
  data: CreateInformationData;
}

export interface UpdateInformationContext {
  id: string;
  groupUid: string;
}

export type UpdateInformationUserContext = UpdateInformationContext;

export interface UpdateInformationData {
  title?: string | null;
  text?: string | null;
  feedType?: string | null;
  feedFile?: string | null;
  feedPic?: string | null;
  feedPic2?: string | null;
  feedPic3?: string | null;
  feedPic4?: string | null;
  eventLocation?: string | null;
  glat?: string | null;
  glong?: string | null;
}

export interface UpdateInformationParams {
  context: UpdateInformationContext;
  data: UpdateInformationData;
}

export interface InformationRepository {
  getInformation(groupUid: string, filter?: InformationFilterParams): Promise<InformationEntity[]>;
  getInformationById(id: string, groupUid: string): Promise<InformationEntity | null>;
  getInformationByIds(ids: string[], groupUid: string): Promise<InformationEntity[]>;
  getFeedCreatorContext(groupUid: string, userEmail: string): Promise<FeedCreatorContext>;
  createInformation(params: CreateInformationParams): Promise<InformationEntity>;
  updateInformation(params: UpdateInformationParams): Promise<InformationEntity>;
  deleteManyInformation(ids: string[], groupUid: string): Promise<number>;
}
