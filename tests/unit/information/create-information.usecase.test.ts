import { InformationEntity } from '@features/information/domain/entities/information.entity';
import type {
  CreateInformationParams,
  FeedCreatorContext,
  InformationFilterParams,
  InformationRepository,
  UpdateInformationParams,
} from '@features/information/domain/repositories/information.repository';
import { CreateInformationUseCase } from '@features/information/usecases/create-information.usecase';
import { describe, expect, it, vi } from 'vitest';

class MockInformationRepository implements InformationRepository {
  public createInformationFn = vi.fn();
  public getFeedCreatorContextFn = vi.fn().mockResolvedValue({
    groupName: 'Veranda Harmoni',
    userProfilePic: 'https://cdn.veranda.com/profile.jpg',
  });
  public getInformationFn = vi.fn();
  public getInformationByIdFn = vi.fn();
  public updateInformationFn = vi.fn();
  public getInformationByIdsFn = vi.fn();
  public deleteManyInformationFn = vi.fn();

  async getInformation(
    groupUid: string,
    filter?: InformationFilterParams,
  ): Promise<InformationEntity[]> {
    return this.getInformationFn(groupUid, filter);
  }

  async getInformationById(id: string, groupUid: string): Promise<InformationEntity | null> {
    return this.getInformationByIdFn(id, groupUid);
  }

  async getInformationByIds(ids: string[], groupUid: string): Promise<InformationEntity[]> {
    return this.getInformationByIdsFn(ids, groupUid);
  }

  async deleteManyInformation(ids: string[], groupUid: string): Promise<number> {
    return this.deleteManyInformationFn(ids, groupUid);
  }

  async getFeedCreatorContext(groupUid: string, userEmail: string): Promise<FeedCreatorContext> {
    return this.getFeedCreatorContextFn(groupUid, userEmail);
  }

  async createInformation(params: CreateInformationParams): Promise<InformationEntity> {
    return this.createInformationFn(params);
  }

  async updateInformation(params: UpdateInformationParams): Promise<InformationEntity> {
    return this.updateInformationFn(params);
  }
}

describe('CreateInformationUseCase Unit Tests', () => {
  it('berhasil membuat informasi jika role adalah ADMIN', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.createInformationFn.mockResolvedValue(
      new InformationEntity({
        id: '1',
        title: 'Pengumuman Penting',
        text: 'Besok ada kerja bakti',
        userEmail: 'admin@veranda.com',
        userName: 'Admin Veranda',
      }),
    );

    const useCase = new CreateInformationUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: {
        email: 'admin@veranda.com',
        name: 'Admin Veranda',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
      data: {
        title: 'Pengumuman Penting',
        text: 'Besok ada kerja bakti',
        feedType: 'EVENT',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value.id).toBe('1');
    expect(result.value.title).toBe('Pengumuman Penting');
    expect(mockRepo.createInformationFn).toHaveBeenCalledWith(
      expect.objectContaining({
        system: expect.objectContaining({
          feedUid: expect.stringMatching(/^feed_[0-9a-f-]{36}$/),
          feedDt: expect.any(Date),
          feedGroupName: 'Veranda Harmoni',
          feedByProfilePic: 'https://cdn.veranda.com/profile.jpg',
        }),
        context: expect.objectContaining({
          groupUid: 'GRP-01',
          creatorEmail: 'admin@veranda.com',
          creatorName: 'Admin Veranda',
        }),
        data: expect.objectContaining({
          title: 'Pengumuman Penting',
          text: 'Besok ada kerja bakti',
          feedType: 'EVENT',
        }),
      }),
    );
  });

  it('berhasil membuat informasi jika role adalah GUARD dan feedType default ke OTHER jika tidak diberikan', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.createInformationFn.mockResolvedValue(
      new InformationEntity({
        id: '2',
        title: 'Laporan Patroli',
        text: 'Kondisi pos 1 aman',
        userEmail: 'guard@veranda.com',
        userName: 'Petugas Jaga',
      }),
    );

    const useCase = new CreateInformationUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: {
        email: 'guard@veranda.com',
        name: 'Petugas Jaga',
        role: 'GUARD',
        group_uid: 'GRP-01',
      },
      data: {
        title: 'Laporan Patroli',
        text: 'Kondisi pos 1 aman',
        feedType: undefined,
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value.id).toBe('2');
    expect(mockRepo.createInformationFn).toHaveBeenCalledWith(
      expect.objectContaining({
        system: expect.objectContaining({
          feedUid: expect.stringMatching(/^feed_[0-9a-f-]{36}$/),
          feedDt: expect.any(Date),
        }),
        context: expect.objectContaining({
          groupUid: 'GRP-01',
          creatorEmail: 'guard@veranda.com',
          creatorName: 'Petugas Jaga',
        }),
        data: expect.objectContaining({
          feedType: 'OTHER',
        }),
      }),
    );
  });

  it('berhasil memecah array photos ke feedPic, feedPic2, feedPic3, feedPic4 di usecase', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.createInformationFn.mockResolvedValue(
      new InformationEntity({
        id: '3',
        title: 'Laporan Foto',
      }),
    );

    const useCase = new CreateInformationUseCase(mockRepo);
    await useCase.execute({
      currentUser: {
        email: 'guard@veranda.com',
        name: 'Petugas Jaga',
        role: 'GUARD',
        group_uid: 'GRP-01',
      },
      data: {
        title: 'Laporan Foto',
        photos: ['p1.jpg', 'p2.jpg', 'p3.jpg', 'p4.jpg'],
      },
    });

    expect(mockRepo.createInformationFn).toHaveBeenCalledWith(
      expect.objectContaining({
        data: expect.objectContaining({
          feedPic: 'p1.jpg',
          feedPic2: 'p2.jpg',
          feedPic3: 'p3.jpg',
          feedPic4: 'p4.jpg',
        }),
      }),
    );
  });

  it('gagal (Bad Request) jika group_uid belum ada', async () => {
    const mockRepo = new MockInformationRepository();
    const useCase = new CreateInformationUseCase(mockRepo);

    const result = await useCase.execute({
      currentUser: {
        email: 'admin@veranda.com',
        name: 'Admin',
        role: 'ADMIN',
        group_uid: null,
      },
      data: {
        title: 'Info',
        feedType: 'GENERAL',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
    expect(result.failure.message).toContain('grup aktif');
  });

  it('gagal (Bad Request) jika email personil tidak ada', async () => {
    const mockRepo = new MockInformationRepository();
    const useCase = new CreateInformationUseCase(mockRepo);

    const result = await useCase.execute({
      currentUser: {
        email: '',
        name: 'Admin',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
      data: {
        title: 'Info',
        feedType: 'GENERAL',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
    expect(result.failure.message).toContain('Data personil (email dan nama) tidak valid');
  });

  it('gagal (Bad Request) jika nama personil tidak ada', async () => {
    const mockRepo = new MockInformationRepository();
    const useCase = new CreateInformationUseCase(mockRepo);

    const result = await useCase.execute({
      currentUser: {
        email: 'admin@veranda.com',
        name: '',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
      data: {
        title: 'Info',
        feedType: 'GENERAL',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
    expect(result.failure.message).toContain('Data personil (email dan nama) tidak valid');
  });

  it('gagal (Bad Request) jika judul informasi tidak diisi', async () => {
    const mockRepo = new MockInformationRepository();
    const useCase = new CreateInformationUseCase(mockRepo);

    const result = await useCase.execute({
      currentUser: {
        email: 'admin@veranda.com',
        name: 'Admin',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
      data: {
        title: '',
        feedType: 'GENERAL',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
    expect(result.failure.message).toContain('Judul informasi wajib diisi');
  });

  it('gagal jika feed_type di luar enum yang diizinkan', async () => {
    const mockRepo = new MockInformationRepository();
    const useCase = new CreateInformationUseCase(mockRepo);

    const result = await useCase.execute({
      currentUser: {
        email: 'admin@veranda.com',
        name: 'Admin',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
      data: {
        title: 'Info',
        feedType: 'INVALID_TYPE',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
    expect(result.failure.message).toContain('feed_type');
  });
});
