import { GuestEntity } from '@features/guest/domain/entities/guest.entity';
import type {
  MasterGuestTypeItem,
  MasterHostItem,
} from '@features/guest/domain/entities/master-data.entity';
import type {
  CreateGuestParams,
  GuestFilterParams,
  GuestRepository,
  UpdateGuestParams,
} from '@features/guest/domain/repositories/guest.repository';
import { DeleteGuestUseCase } from '@features/guest/usecases/delete-guest.usecase';
import { describe, expect, it, vi } from 'vitest';

class MockGuestRepository implements GuestRepository {
  public createGuestFn = vi.fn();
  public getGuestsFn = vi.fn();
  public getGuestByIdFn = vi.fn();
  public getGuestsByIdsFn = vi.fn();
  public updateGuestFn = vi.fn();
  public deleteManyGuestsFn = vi.fn();
  public isValidGuestTypeFn = vi.fn();
  public getMasterGuestTypesFn = vi.fn();
  public getMasterHostsFn = vi.fn();

  async getGuests(groupUid: string, filter?: GuestFilterParams): Promise<GuestEntity[]> {
    return this.getGuestsFn(groupUid, filter);
  }

  async getGuestById(id: string, groupUid: string): Promise<GuestEntity | null> {
    return this.getGuestByIdFn(id, groupUid);
  }

  async getGuestsByIds(ids: string[], groupUid: string): Promise<GuestEntity[]> {
    return this.getGuestsByIdsFn(ids, groupUid);
  }

  async createGuest(params: CreateGuestParams): Promise<GuestEntity> {
    return this.createGuestFn(params);
  }

  async updateGuest(params: UpdateGuestParams): Promise<GuestEntity> {
    return this.updateGuestFn(params);
  }

  async deleteManyGuests(ids: string[], groupUid: string): Promise<number> {
    return this.deleteManyGuestsFn(ids, groupUid);
  }

  async isValidGuestType(guestType: string, groupUid: string): Promise<boolean> {
    return this.isValidGuestTypeFn(guestType, groupUid);
  }

  async resolveCanonicalGuestType(guestType: string, _groupUid: string): Promise<string | null> {
    return guestType;
  }

  async getMasterHosts(groupUid: string): Promise<MasterHostItem[]> {
    return this.getMasterHostsFn(groupUid);
  }

  async getMasterGuestTypes(groupUid: string): Promise<MasterGuestTypeItem[]> {
    return this.getMasterGuestTypesFn(groupUid);
  }

  public isValidHostFn = vi.fn().mockResolvedValue(true);
  async isValidHost(email: string, groupUid: string): Promise<boolean> {
    return this.isValidHostFn(email, groupUid);
  }
}

describe('DeleteGuestUseCase Unit Tests', () => {
  const existingGuestFromGuard = new GuestEntity({
    id: '10',
    guestUid: 'GST-010',
    groupUid: 'GRP-01',
    guestName: 'Tamu Guard',
    guestDestination: 'Blok C1',
    guestCreatorEmail: 'guard1@veranda.com',
    guestCreatorName: 'Satpam Satu',
  });

  const existingGuestFromOther = new GuestEntity({
    id: '20',
    guestUid: 'GST-020',
    groupUid: 'GRP-01',
    guestName: 'Tamu Orang Lain',
    guestDestination: 'Blok D2',
    guestCreatorEmail: 'someoneelse@veranda.com',
    guestCreatorName: 'Orang Lain',
  });

  // === Single Delete Tests ===

  it('ADMIN dapat menghapus data tamu mana pun dalam grup aktif (single)', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestsByIdsFn.mockResolvedValue([existingGuestFromOther]);
    mockRepo.deleteManyGuestsFn.mockResolvedValue(1);

    const useCase = new DeleteGuestUseCase(mockRepo);
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
    expect(mockRepo.deleteManyGuestsFn).toHaveBeenCalledWith(['20'], 'GRP-01');
  });

  it('GUARD dapat menghapus data tamu miliknya sendiri (single)', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestsByIdsFn.mockResolvedValue([existingGuestFromGuard]);
    mockRepo.deleteManyGuestsFn.mockResolvedValue(1);

    const useCase = new DeleteGuestUseCase(mockRepo);
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
    expect(mockRepo.deleteManyGuestsFn).toHaveBeenCalledWith(['10'], 'GRP-01');
  });

  it('GUARD ditolak (403 Forbidden) jika menghapus data tamu milik orang lain (single)', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestsByIdsFn.mockResolvedValue([existingGuestFromOther]);

    const useCase = new DeleteGuestUseCase(mockRepo);
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
    expect(mockRepo.deleteManyGuestsFn).not.toHaveBeenCalled();
  });

  it('Mengembalikan 404 Not Found jika data tamu tidak ditemukan (single)', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestsByIdsFn.mockResolvedValue([]);

    const useCase = new DeleteGuestUseCase(mockRepo);
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
    expect(mockRepo.deleteManyGuestsFn).not.toHaveBeenCalled();
  });

  // === Batch Delete Tests ===

  it('ADMIN dapat menghapus banyak data tamu sekaligus (batch)', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestsByIdsFn.mockResolvedValue([existingGuestFromGuard, existingGuestFromOther]);
    mockRepo.deleteManyGuestsFn.mockResolvedValue(2);

    const useCase = new DeleteGuestUseCase(mockRepo);
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
    expect(mockRepo.getGuestsByIdsFn).toHaveBeenCalledWith(['10', '20'], 'GRP-01');
    expect(mockRepo.deleteManyGuestsFn).toHaveBeenCalledWith(['10', '20'], 'GRP-01');
  });

  it('GUARD ditolak (403) saat batch berisi tamu milik orang lain', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestsByIdsFn.mockResolvedValue([existingGuestFromGuard, existingGuestFromOther]);

    const useCase = new DeleteGuestUseCase(mockRepo);
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
    expect(mockRepo.deleteManyGuestsFn).not.toHaveBeenCalled();
  });

  it('Mengembalikan 404 jika sebagian ID batch tidak ditemukan', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestsByIdsFn.mockResolvedValue([existingGuestFromGuard]);

    const useCase = new DeleteGuestUseCase(mockRepo);
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
    expect(mockRepo.deleteManyGuestsFn).not.toHaveBeenCalled();
  });

  it('Mengembalikan validation error jika IDs kosong', async () => {
    const mockRepo = new MockGuestRepository();
    const useCase = new DeleteGuestUseCase(mockRepo);

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
    expect(mockRepo.getGuestsByIdsFn).not.toHaveBeenCalled();
  });

  it('Mengembalikan validation error jika group_uid tidak ada', async () => {
    const mockRepo = new MockGuestRepository();
    const useCase = new DeleteGuestUseCase(mockRepo);

    const result = await useCase.execute({
      ids: ['10'],
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: null,
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
  });

  it('Mendeduplikasi ID yang sama dalam satu batch', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestsByIdsFn.mockResolvedValue([existingGuestFromGuard]);
    mockRepo.deleteManyGuestsFn.mockResolvedValue(1);

    const useCase = new DeleteGuestUseCase(mockRepo);
    const result = await useCase.execute({
      ids: ['10', '10', '10'],
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(mockRepo.getGuestsByIdsFn).toHaveBeenCalledWith(['10'], 'GRP-01');
  });
});
