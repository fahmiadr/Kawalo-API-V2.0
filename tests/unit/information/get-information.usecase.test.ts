import { InformationEntity } from '@features/information/domain/entities/information.entity';
import type {
  CreateInformationParams,
  FeedCreatorContext,
  InformationFilterParams,
  InformationRepository,
  UpdateInformationParams,
} from '@features/information/domain/repositories/information.repository';
import { GetInformationUseCase } from '@features/information/usecases/get-information.usecase';
import { InformationResponseMapper } from '@features/information/presentation/mappers/information-response.mapper';
import { describe, expect, it } from 'vitest';

class MockInformationRepository implements InformationRepository {
  public data: InformationEntity[] = [];

  async getInformation(
    _groupUid: string,
    _filter?: InformationFilterParams,
  ): Promise<InformationEntity[]> {
    return this.data;
  }

  async getInformationById(id: string, _groupUid: string): Promise<InformationEntity | null> {
    return this.data.find((item) => item.id === id) ?? null;
  }

  async getFeedCreatorContext(_groupUid: string, _userEmail: string): Promise<FeedCreatorContext> {
    return { groupName: 'Grup Default', userProfilePic: null };
  }

  async createInformation(params: CreateInformationParams): Promise<InformationEntity> {
    const entity = new InformationEntity({
      id: '1',
      title: params.data.title,
      text: params.data.text,
      userEmail: params.context.creatorEmail,
      userName: params.context.creatorName,
      groupName: params.system.feedGroupName,
      userProfilePic: params.system.feedByProfilePic,
      feedDt: params.system.feedDt.toISOString(),
    });
    this.data.push(entity);
    return entity;
  }

  async updateInformation(params: UpdateInformationParams): Promise<InformationEntity> {
    const existing = this.data.find((item) => item.id === params.context.id);
    if (!existing) throw new Error('Not found');
    return existing;
  }

  async getInformationByIds(ids: string[], _groupUid: string): Promise<InformationEntity[]> {
    return this.data.filter((item) => ids.includes(item.id));
  }

  async deleteInformation(id: string, _groupUid: string): Promise<boolean> {
    this.data = this.data.filter((item) => item.id !== id);
    return true;
  }

  async deleteManyInformation(ids: string[], _groupUid: string): Promise<number> {
    const initialLen = this.data.length;
    this.data = this.data.filter((item) => !ids.includes(item.id));
    return initialLen - this.data.length;
  }
}

describe('Information UseCase Unit Tests', () => {
  const dummyFeed = new InformationEntity({
    id: '101',
    title: 'Informasi Penjagaan Pos Utama',
    text: 'Shift malam akan ditambah 2 personil.',
    userEmail: 'author@kawalo.id',
    userName: 'Admin Security',
    userProfilePic: 'https://cdn.kawalo.id/avatar1.png',
    feedDt: '2026-09-15T10:00:00.000Z',
    feedFile: 'sop.pdf',
    photos: ['https://cdn.kawalo.id/feed1.jpg', 'https://cdn.kawalo.id/feed2.jpg'],
    feedType: 'GENERAL',
    groupName: 'Tim Pengamanan Blok A',
    eventLocation: 'Pos 1 Gerbang Utama',
    eventStartDt: '2026-09-15T19:00:00.000Z',
    eventEndDt: '2026-09-15T23:00:00.000Z',
    eventDuration: 4,
    glat: '-6.200000',
    glong: '106.816666',
  });

  it('GetInformationUseCase berhasil mengembalikan list informasi terformat', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.data = [dummyFeed];

    const usecase = new GetInformationUseCase(mockRepo);
    const result = await usecase.execute({
      currentUser: { group_uid: 'group-security-a' },
    });

    expect(result.isSuccess()).toBe(true);
    const items = result.value;
    expect(items).toHaveLength(1);
    expect(items[0]?.id).toBe('101');
    expect(items[0]?.title).toBe('Informasi Penjagaan Pos Utama');
    expect(items[0]?.userEmail).toBe('author@kawalo.id');
    expect(items[0]?.userName).toBe('Admin Security');
    expect(items[0]?.photos).toEqual([
      'https://cdn.kawalo.id/feed1.jpg',
      'https://cdn.kawalo.id/feed2.jpg',
    ]);
    expect(items[0]?.glat).toBe('-6.200000');
    expect(items[0]?.glong).toBe('106.816666');
    expect(items[0]?.feedType).toBe('GENERAL');

    // Verifikasi pemetaan Presentation via InformationResponseMapper
    const firstInfo = items[0];
    expect(firstInfo).toBeDefined();
    if (!firstInfo) throw new Error('Information tidak ditemukan');
    const mapped = InformationResponseMapper.toResponse(firstInfo);
    expect(mapped.user_email).toBe('author@kawalo.id');
    expect(mapped.user_name).toBe('Admin Security');
    expect(mapped.feed_type).toBe('GENERAL');
  });

  it('mengembalikan array kosong jika pengguna belum memiliki grup aktif', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.data = [dummyFeed];

    const usecase = new GetInformationUseCase(mockRepo);
    const result = await usecase.execute({
      currentUser: { group_uid: null },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value).toHaveLength(0);
  });

  it('mengizinkan feed_type bernilai null tanpa fallback paksa', async () => {
    const unclassifiedFeed = new InformationEntity({
      id: '102',
      title: 'Feed Tanpa Kategori',
      userEmail: 'author@kawalo.id',
      feedDt: '2026-09-15T11:00:00.000Z',
    });

    const mockRepo = new MockInformationRepository();
    mockRepo.data = [unclassifiedFeed];

    const usecase = new GetInformationUseCase(mockRepo);
    const result = await usecase.execute({
      currentUser: { group_uid: 'group-security-a' },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value[0]?.feedType).toBeNull();
  });
});
