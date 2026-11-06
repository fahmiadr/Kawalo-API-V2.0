import { InformationEntity } from '@features/information/domain/entities/information.entity';
import type {
  CreateInformationParams,
  FeedCreatorContext,
  InformationFilterParams,
  InformationRepository,
  UpdateInformationParams,
} from '@features/information/domain/repositories/information.repository';
import { UpdateInformationUseCase } from '@features/information/usecases/update-information.usecase';
import { describe, expect, it, vi } from 'vitest';

class MockInformationRepository implements InformationRepository {
  public createInformationFn = vi.fn();
  public getInformationFn = vi.fn();
  public getInformationByIdFn = vi.fn();
  public updateInformationFn = vi.fn();
  public deleteInformationFn = vi.fn();

  async getInformation(
    groupUid: string,
    filter?: InformationFilterParams,
  ): Promise<InformationEntity[]> {
    return this.getInformationFn(groupUid, filter);
  }

  async getInformationById(id: string, groupUid: string): Promise<InformationEntity | null> {
    return this.getInformationByIdFn(id, groupUid);
  }

  async getFeedCreatorContext(_groupUid: string, _userEmail: string): Promise<FeedCreatorContext> {
    return { groupName: 'Grup Default', userProfilePic: null };
  }

  async createInformation(params: CreateInformationParams): Promise<InformationEntity> {
    return this.createInformationFn(params);
  }

  async updateInformation(params: UpdateInformationParams): Promise<InformationEntity> {
    return this.updateInformationFn(params);
  }

  async deleteInformation(id: string, groupUid: string): Promise<boolean> {
    return this.deleteInformationFn(id, groupUid);
  }

  async getInformationByIds(ids: string[], groupUid: string): Promise<InformationEntity[]> {
    return [];
  }

  async deleteManyInformation(ids: string[], groupUid: string): Promise<number> {
    return 0;
  }
}

describe('UpdateInformationUseCase Unit Tests', () => {
  const existingFeedFromGuard = new InformationEntity({
    id: '10',
    title: 'Info dari Guard',
    text: 'Patroli selesai',
    userEmail: 'guard1@veranda.com',
    userName: 'Satpam Satu',
  });

  const existingFeedFromOther = new InformationEntity({
    id: '20',
    title: 'Info dari Orang Lain',
    text: 'Pengumuman warga',
    userEmail: 'someoneelse@veranda.com',
    userName: 'Orang Lain',
  });

  it('ADMIN dapat mengubah informasi mana pun dalam grup aktif', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.getInformationByIdFn.mockResolvedValue(existingFeedFromOther);
    mockRepo.updateInformationFn.mockResolvedValue(
      new InformationEntity({
        ...existingFeedFromOther,
        title: 'Judul Diperbarui Admin',
      }),
    );

    const useCase = new UpdateInformationUseCase(mockRepo);
    const result = await useCase.execute({
      id: '20',
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
      data: {
        title: 'Judul Diperbarui Admin',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value.title).toBe('Judul Diperbarui Admin');
    expect(mockRepo.updateInformationFn).toHaveBeenCalledWith(
      expect.objectContaining({
        context: expect.objectContaining({
          id: '20',
          groupUid: 'GRP-01',
        }),
        data: expect.objectContaining({
          title: 'Judul Diperbarui Admin',
        }),
      }),
    );
  });

  it('GUARD dapat mengubah informasi miliknya sendiri', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.getInformationByIdFn.mockResolvedValue(existingFeedFromGuard);
    mockRepo.updateInformationFn.mockResolvedValue(
      new InformationEntity({
        ...existingFeedFromGuard,
        title: 'Judul Diperbarui Guard Sendiri',
      }),
    );

    const useCase = new UpdateInformationUseCase(mockRepo);
    const result = await useCase.execute({
      id: '10',
      currentUser: {
        email: 'guard1@veranda.com',
        role: 'GUARD',
        group_uid: 'GRP-01',
      },
      data: {
        title: 'Judul Diperbarui Guard Sendiri',
        feedType: 'INCIDENT',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value.title).toBe('Judul Diperbarui Guard Sendiri');
    expect(mockRepo.updateInformationFn).toHaveBeenCalledWith(
      expect.objectContaining({
        context: expect.objectContaining({
          id: '10',
          groupUid: 'GRP-01',
        }),
        data: expect.objectContaining({
          title: 'Judul Diperbarui Guard Sendiri',
          feedType: 'INCIDENT',
        }),
      }),
    );
  });

  it('berhasil memecah array photos ke feedPic, feedPic2, feedPic3, feedPic4 di usecase saat update', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.getInformationByIdFn.mockResolvedValue(existingFeedFromOther);
    mockRepo.updateInformationFn.mockResolvedValue(
      new InformationEntity({
        ...existingFeedFromOther,
        title: 'Update Photos',
      }),
    );

    const useCase = new UpdateInformationUseCase(mockRepo);
    await useCase.execute({
      id: '20',
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
      data: {
        photos: ['p1.jpg', 'p2.jpg'],
      },
    });

    expect(mockRepo.updateInformationFn).toHaveBeenCalledWith(
      expect.objectContaining({
        context: {
          id: '20',
          groupUid: 'GRP-01',
        },
        data: expect.objectContaining({
          feedPic: 'p1.jpg',
          feedPic2: 'p2.jpg',
          feedPic3: null,
          feedPic4: null,
        }),
      }),
    );
  });

  it('GUARD ditolak (403 Forbidden) jika mencoba mengubah informasi milik orang lain', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.getInformationByIdFn.mockResolvedValue(existingFeedFromOther);

    const useCase = new UpdateInformationUseCase(mockRepo);
    const result = await useCase.execute({
      id: '20',
      currentUser: {
        email: 'guard1@veranda.com',
        role: 'GUARD',
        group_uid: 'GRP-01',
      },
      data: {
        title: 'Coba Edit',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Forbidden');
    expect(mockRepo.updateInformationFn).not.toHaveBeenCalled();
  });

  it('Mengembalikan Bad Request jika group_uid belum ada', async () => {
    const mockRepo = new MockInformationRepository();
    const useCase = new UpdateInformationUseCase(mockRepo);

    const result = await useCase.execute({
      id: '10',
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: null,
      },
      data: { title: 'No Group' },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
  });

  it('Mengembalikan Bad Request jika ID informasi tidak diisi', async () => {
    const mockRepo = new MockInformationRepository();
    const useCase = new UpdateInformationUseCase(mockRepo);

    const result = await useCase.execute({
      id: '',
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
      data: { title: 'Empty ID' },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
  });

  it('Mengembalikan 404 Not Found jika data informasi tidak ditemukan', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.getInformationByIdFn.mockResolvedValue(null);

    const useCase = new UpdateInformationUseCase(mockRepo);
    const result = await useCase.execute({
      id: '999',
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
      data: { title: 'Tak Ada' },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Not Found');
    expect(mockRepo.updateInformationFn).not.toHaveBeenCalled();
  });

  it('gagal jika feedType yang diupdate di luar enum allowed', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.getInformationByIdFn.mockResolvedValue(existingFeedFromOther);

    const useCase = new UpdateInformationUseCase(mockRepo);
    const result = await useCase.execute({
      id: '20',
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
      data: { feedType: 'INVALID_TYPE' },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
    expect(result.failure.message).toContain('feed_type');
  });

  it('berhasil meneruskan nilai null untuk mengosongkan text, feedFile, eventLocation, glat, glong, dan photos', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.getInformationByIdFn.mockResolvedValue(existingFeedFromOther);
    mockRepo.updateInformationFn.mockResolvedValue(
      new InformationEntity({
        ...existingFeedFromOther,
        text: null,
        feedFile: null,
        eventLocation: null,
        glat: null,
        glong: null,
      }),
    );

    const useCase = new UpdateInformationUseCase(mockRepo);
    const result = await useCase.execute({
      id: '20',
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
      data: {
        text: null,
        feedFile: null,
        eventLocation: null,
        glat: null,
        glong: null,
        photos: [],
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(mockRepo.updateInformationFn).toHaveBeenCalledWith(
      expect.objectContaining({
        context: {
          id: '20',
          groupUid: 'GRP-01',
        },
        data: expect.objectContaining({
          text: null,
          feedFile: null,
          eventLocation: null,
          glat: null,
          glong: null,
          feedPic: null,
          feedPic2: null,
          feedPic3: null,
          feedPic4: null,
        }),
      }),
    );
  });
});

