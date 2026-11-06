import type { MasterGuestTypeItem } from '@features/guest/domain/entities/master-data.entity';
import type {
  CreateGuestParams,
  GuestFilterParams,
  GuestRepository,
  UpdateGuestParams,
} from '@features/guest/domain/repositories/guest.repository';
import { GetMasterGuestTypesUseCase } from '@features/guest/usecases/get-master-guest-types.usecase';
import { describe, expect, it, vi } from 'vitest';

class MockGuestRepository implements GuestRepository {
  public createGuestFn = vi.fn();
  public getGuestsFn = vi.fn();
  public getGuestByIdFn = vi.fn();
  public updateGuestFn = vi.fn();
  public isValidGuestTypeFn = vi.fn();
  public getMasterGuestTypesFn = vi.fn();

  async getGuests(groupUid: string, filter?: GuestFilterParams) {
    return this.getGuestsFn(groupUid, filter);
  }

  async getGuestById(id: string, groupUid: string) {
    return this.getGuestByIdFn(id, groupUid);
  }

  async createGuest(params: CreateGuestParams) {
    return this.createGuestFn(params);
  }

  async updateGuest(params: UpdateGuestParams) {
    return this.updateGuestFn(params);
  }

  public getGuestsByIdsFn = vi.fn();
  async getGuestsByIds(ids: string[], groupUid: string) {
    return this.getGuestsByIdsFn(ids, groupUid);
  }

  public deleteManyGuestsFn = vi.fn();
  async deleteManyGuests(ids: string[], groupUid: string) {
    return this.deleteManyGuestsFn(ids, groupUid);
  }

  async isValidGuestType(guestType: string, groupUid: string) {
    return this.isValidGuestTypeFn(guestType, groupUid);
  }

  async resolveCanonicalGuestType(guestType: string, _groupUid: string): Promise<string | null> {
    return guestType;
  }

  async getMasterGuestTypes(groupUid: string): Promise<MasterGuestTypeItem[]> {
    return this.getMasterGuestTypesFn(groupUid);
  }

  public isValidHostFn = vi.fn().mockResolvedValue(true);
  async isValidHost(email: string, groupUid: string): Promise<boolean> {
    return this.isValidHostFn(email, groupUid);
  }
}

describe('GetMasterGuestTypesUseCase Unit Tests', () => {
  it('berhasil mengambil master tipe tamu sesuai group aktif', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getMasterGuestTypesFn.mockResolvedValue([
      { id: '1', uid: 'TYPE-01', name: 'TAMU UMUM' },
      { id: '2', uid: 'TYPE-02', name: 'KURIR' },
      { id: '3', uid: 'TYPE-03', name: 'KONTRAKTOR' },
    ]);

    const useCase = new GetMasterGuestTypesUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: {
        group_uid: 'GRP-01',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value).toHaveLength(3);
    expect(result.value[0]?.name).toBe('TAMU UMUM');
    expect(mockRepo.getMasterGuestTypesFn).toHaveBeenCalledWith('GRP-01');
  });

  it('mengembalikan array kosong jika group_uid kosong', async () => {
    const mockRepo = new MockGuestRepository();

    const useCase = new GetMasterGuestTypesUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: {
        group_uid: null,
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value).toEqual([]);
    expect(mockRepo.getMasterGuestTypesFn).not.toHaveBeenCalled();
  });
});
