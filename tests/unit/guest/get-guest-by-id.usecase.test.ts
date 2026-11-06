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
import { GetGuestByIdUseCase } from '@features/guest/usecases/get-guest-by-id.usecase';
import { describe, expect, it, vi } from 'vitest';

class MockGuestRepository implements GuestRepository {
  public getGuestByIdFn = vi.fn();

  async getGuests(_groupUid: string, _filter?: GuestFilterParams): Promise<GuestEntity[]> {
    return [];
  }

  async getGuestById(id: string, groupUid: string): Promise<GuestEntity | null> {
    return this.getGuestByIdFn(id, groupUid);
  }

  async getGuestsByIds(_ids: string[], _groupUid: string): Promise<GuestEntity[]> {
    return [];
  }

  async createGuest(_params: CreateGuestParams): Promise<GuestEntity> {
    throw new Error('Not implemented');
  }

  async updateGuest(_params: UpdateGuestParams): Promise<GuestEntity> {
    throw new Error('Not implemented');
  }

  async deleteManyGuests(_ids: string[], _groupUid: string): Promise<number> {
    return 0;
  }

  async isValidGuestType(_guestType: string, _groupUid: string): Promise<boolean> {
    return true;
  }

  async resolveCanonicalGuestType(guestType: string, _groupUid: string): Promise<string | null> {
    return guestType;
  }

  async isValidHost(_email: string, _groupUid: string): Promise<boolean> {
    return true;
  }

  async getMasterGuestTypes(_groupUid: string): Promise<MasterGuestTypeItem[]> {
    return [];
  }

  async getMasterHosts(_groupUid: string): Promise<MasterHostItem[]> {
    return [];
  }
}

describe('GetGuestByIdUseCase Unit Tests', () => {
  const sampleGuest = new GuestEntity({
    id: '10',
    qrUid: 'QR-VER-12345',
    guestUid: 'GST-UID-001',
    groupUid: 'GRP-01',
    groupName: 'Cluster Bougenville',
    guestName: 'Bambang Sudarmono',
    guestDestination: 'Blok B No 12',
    guestType: 'Tamu Pribadi',
    guestDt: '2026-10-09T08:00:00.000Z',
    guestAckDt: null,
    guestOutDt: null,
  });

  it('mengembalikan Result.ok(null) jika pengguna belum memilih grup aktif (group_uid kosong)', async () => {
    const mockRepo = new MockGuestRepository();
    const useCase = new GetGuestByIdUseCase(mockRepo);

    const result = await useCase.execute({
      id: '10',
      currentUser: { group_uid: null },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value).toBeNull();
    expect(mockRepo.getGuestByIdFn).not.toHaveBeenCalled();
  });

  it('gagal jika ID atau QR tamu kosong', async () => {
    const mockRepo = new MockGuestRepository();
    const useCase = new GetGuestByIdUseCase(mockRepo);

    const result = await useCase.execute({
      id: '',
      currentUser: { group_uid: 'GRP-01' },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
    expect(result.failure.message).toContain('ID atau QR tamu wajib diisi');
    expect(mockRepo.getGuestByIdFn).not.toHaveBeenCalled();
  });

  it('gagal (NotFound) jika data tamu tidak ditemukan dalam grup aktif', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(null);
    const useCase = new GetGuestByIdUseCase(mockRepo);

    const result = await useCase.execute({
      id: '999',
      currentUser: { group_uid: 'GRP-01' },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Not Found');
    expect(result.failure.message).toContain('Data tamu tidak ditemukan');
    expect(mockRepo.getGuestByIdFn).toHaveBeenCalledWith('999', 'GRP-01');
  });

  it('berhasil mengambil data tamu berdasarkan numeric ID', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(sampleGuest);
    const useCase = new GetGuestByIdUseCase(mockRepo);

    const result = await useCase.execute({
      id: '10',
      currentUser: { group_uid: 'GRP-01' },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value?.id).toBe('10');
    expect(result.value?.guestName).toBe('Bambang Sudarmono');
    expect(mockRepo.getGuestByIdFn).toHaveBeenCalledWith('10', 'GRP-01');
  });

  it('berhasil mengambil data tamu berdasarkan QRUID hasil scan', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(sampleGuest);
    const useCase = new GetGuestByIdUseCase(mockRepo);

    const result = await useCase.execute({
      id: 'QR-VER-12345',
      currentUser: { group_uid: 'GRP-01' },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value?.qrUid).toBe('QR-VER-12345');
    expect(result.value?.guestName).toBe('Bambang Sudarmono');
    expect(mockRepo.getGuestByIdFn).toHaveBeenCalledWith('QR-VER-12345', 'GRP-01');
  });
});
