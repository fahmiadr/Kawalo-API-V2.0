import { InformationEntity } from '@features/information/domain/entities/information.entity';
import type {
  CreateInformationParams,
  FeedCreatorContext,
  InformationFilterParams,
  InformationRepository,
  UpdateInformationParams,
} from '@features/information/domain/repositories/information.repository';
import { DeleteInformationUseCase } from '@features/information/usecases/delete-information.usecase';
import { describe, expect, it, vi } from 'vitest';

class MockInformationRepository implements InformationRepository {
  public createInformationFn = vi.fn();
  public getFeedCreatorContextFn = vi.fn().mockResolvedValue({
    groupName: 'Grup Veranda',
    userProfilePic: null,
  });
  public getInformationFn = vi.fn();
  public getInformationByIdFn = vi.fn();
  public getInformationByIdsFn = vi.fn(async (ids: string[], groupUid: string) => {
    const results: InformationEntity[] = [];
    for (const id of ids) {
      const item = await this.getInformationById(id, groupUid);
      if (item) results.push(item);
    }
    return results;
  });
  public updateInformationFn = vi.fn();
  public deleteInformationFn = vi.fn();
  public deleteManyInformationFn = vi.fn(async (ids: string[], groupUid: string) => {
    let count = 0;
    for (const id of ids) {
      const ok = await this.deleteInformation(id, groupUid);
      if (ok) count++;
    }
    return count;
  });

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

  async getFeedCreatorContext(groupUid: string, userEmail: string): Promise<FeedCreatorContext> {
    return this.getFeedCreatorContextFn(groupUid, userEmail);
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

  async deleteManyInformation(ids: string[], groupUid: string): Promise<number> {
    return this.deleteManyInformationFn(ids, groupUid);
  }
}

describe('DeleteInformationUseCase Unit Tests', () => {
  const existingFeedFromGuard = new InformationEntity({
    id: '10',
    title: 'Info dari Guard',
    userEmail: 'guard1@veranda.com',
    userName: 'Satpam Satu',
  });

  const existingFeedFromGuard2 = new InformationEntity({
    id: '11',
    title: 'Info dari Guard 2',
    userEmail: 'guard1@veranda.com',
    userName: 'Satpam Satu',
  });

  const existingFeedFromOther = new InformationEntity({
    id: '20',
    title: 'Info dari Orang Lain',
    userEmail: 'someoneelse@veranda.com',
    userName: 'Orang Lain',
  });

  it('ADMIN dapat menghapus informasi mana pun dalam grup aktif', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.getInformationByIdFn.mockResolvedValue(existingFeedFromOther);
    mockRepo.deleteInformationFn.mockResolvedValue(true);

    const useCase = new DeleteInformationUseCase(mockRepo);
    const result = await useCase.execute({
      id: '20',
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value).toEqual({ deletedCount: 1 });
  });

  it('GUARD dapat menghapus informasi miliknya sendiri', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.getInformationByIdFn.mockResolvedValue(existingFeedFromGuard);
    mockRepo.deleteInformationFn.mockResolvedValue(true);

    const useCase = new DeleteInformationUseCase(mockRepo);
    const result = await useCase.execute({
      id: '10',
      currentUser: {
        email: 'guard1@veranda.com',
        role: 'GUARD',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value).toEqual({ deletedCount: 1 });
  });

  it('GUARD ditolak (403 Forbidden) jika mencoba menghapus informasi milik orang lain', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.getInformationByIdFn.mockResolvedValue(existingFeedFromOther);

    const useCase = new DeleteInformationUseCase(mockRepo);
    const result = await useCase.execute({
      id: '20',
      currentUser: {
        email: 'guard1@veranda.com',
        role: 'GUARD',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Forbidden');
  });

  it('Mengembalikan 404 Not Found jika data informasi tidak ditemukan', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.getInformationByIdFn.mockResolvedValue(null);

    const useCase = new DeleteInformationUseCase(mockRepo);
    const result = await useCase.execute({
      id: '999',
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Not Found');
  });

  it('ADMIN dapat menghapus batch banyak ID sekaligus', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.getInformationByIdsFn.mockResolvedValue([
      existingFeedFromGuard,
      existingFeedFromOther,
    ]);
    mockRepo.deleteManyInformationFn.mockResolvedValue(2);

    const useCase = new DeleteInformationUseCase(mockRepo);
    const result = await useCase.execute({
      ids: ['10', '20'],
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value).toEqual({ deletedCount: 2 });
    expect(mockRepo.deleteManyInformationFn).toHaveBeenCalledWith(['10', '20'], 'GRP-01');
  });

  it('GUARD berhasil menghapus batch jika seluruh data adalah miliknya sendiri', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.getInformationByIdsFn.mockResolvedValue([
      existingFeedFromGuard,
      existingFeedFromGuard2,
    ]);
    mockRepo.deleteManyInformationFn.mockResolvedValue(2);

    const useCase = new DeleteInformationUseCase(mockRepo);
    const result = await useCase.execute({
      ids: ['10', '11'],
      currentUser: {
        email: 'guard1@veranda.com',
        role: 'GUARD',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value).toEqual({ deletedCount: 2 });
    expect(mockRepo.deleteManyInformationFn).toHaveBeenCalledWith(['10', '11'], 'GRP-01');
  });

  it('GUARD ditolak (403 Forbidden) jika salah satu ID dalam batch adalah milik orang lain', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.getInformationByIdsFn.mockResolvedValue([
      existingFeedFromGuard,
      existingFeedFromOther,
    ]);

    const useCase = new DeleteInformationUseCase(mockRepo);
    const result = await useCase.execute({
      ids: ['10', '20'],
      currentUser: {
        email: 'guard1@veranda.com',
        role: 'GUARD',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Forbidden');
    expect(mockRepo.deleteManyInformationFn).not.toHaveBeenCalled();
  });

  it('Batch delete gagal jika salah satu ID tidak ditemukan dalam grup aktif', async () => {
    const mockRepo = new MockInformationRepository();
    mockRepo.getInformationByIdsFn.mockResolvedValue([existingFeedFromGuard]);

    const useCase = new DeleteInformationUseCase(mockRepo);
    const result = await useCase.execute({
      ids: ['10', '999'],
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Not Found');
    expect(mockRepo.deleteManyInformationFn).not.toHaveBeenCalled();
  });

  it('Gagal jika ID kosong atau tidak valid', async () => {
    const mockRepo = new MockInformationRepository();
    const useCase = new DeleteInformationUseCase(mockRepo);
    const result = await useCase.execute({
      ids: [],
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
  });
});
