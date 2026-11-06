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
import { GuestResponseMapper } from '@features/guest/presentation/mappers/guest-response.mapper';
import { GetGuestsUseCase } from '@features/guest/usecases/get-guests.usecase';
import { describe, expect, it, vi } from 'vitest';

class MockGuestRepository implements GuestRepository {
  public data: GuestEntity[] = [];

  async getGuests(_groupUid: string, _filter?: GuestFilterParams): Promise<GuestEntity[]> {
    return this.data;
  }

  async getGuestById(_id: string, _groupUid: string): Promise<GuestEntity | null> {
    return this.data.find((g) => g.id === _id) ?? null;
  }

  async createGuest(params: CreateGuestParams): Promise<GuestEntity> {
    const entity = new GuestEntity({
      id: '1',
      groupUid: params.context.groupUid,
      guestName: params.data.guestName,
      guestDestination: params.data.guestDestination,
    });
    this.data.push(entity);
    return entity;
  }

  async updateGuest(params: UpdateGuestParams): Promise<GuestEntity> {
    const existing = this.data.find((g) => g.id === params.context.id);
    if (!existing) throw new Error('Not found');
    return existing;
  }

  async getGuestsByIds(ids: string[], _groupUid: string): Promise<GuestEntity[]> {
    return this.data.filter((g) => ids.includes(g.id));
  }

  async deleteManyGuests(ids: string[], _groupUid: string): Promise<number> {
    const before = this.data.length;
    this.data = this.data.filter((g) => !ids.includes(g.id));
    return before - this.data.length;
  }

  async isValidGuestType(_guestType: string, _groupUid: string): Promise<boolean> {
    return true;
  }

  async resolveCanonicalGuestType(guestType: string, _groupUid: string): Promise<string | null> {
    return guestType;
  }

  async getMasterHosts(_groupUid: string): Promise<MasterHostItem[]> {
    return [];
  }

  async getMasterGuestTypes(_groupUid: string): Promise<MasterGuestTypeItem[]> {
    return [];
  }

  async isValidHost(_email: string, _groupUid: string): Promise<boolean> {
    return true;
  }
}

describe('GetGuestsUseCase Unit Tests', () => {
  it('berhasil mengembalikan daftar tamu terformat sesuai grup aktif pengguna', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.data = [
      new GuestEntity({
        id: '1001',
        guestUid: 'GST-001',
        groupUid: 'GRP-01',
        groupName: 'Cluster Sakura',
        guestName: 'Budi Darmawan',
        guestNumber: 'B 1234 CD',
        guestDestination: 'Blok A1 No 5',
        guestCheckOutByEmail: null,
        guestCheckOutByName: null,
      }),
    ];

    const useCase = new GetGuestsUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: { group_uid: 'GRP-01' },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value).toHaveLength(1);
    expect(result.value[0]?.id).toBe('1001');
    expect(result.value[0]?.groupUid).toBe('GRP-01');
    expect(result.value[0]?.guestName).toBe('Budi Darmawan');

    // Verifikasi pemetaan Presentation via GuestResponseMapper
    const firstGuest = result.value[0];
    expect(firstGuest).toBeDefined();
    if (!firstGuest) throw new Error('Guest tidak ditemukan');
    const mapped = GuestResponseMapper.toResponse(firstGuest);
    expect(mapped.group_name).toBe('Cluster Sakura');
    expect(mapped.guest_name).toBe('Budi Darmawan');
  });

  it('mengembalikan array kosong jika pengguna belum memiliki grup aktif', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.data = [
      new GuestEntity({
        id: '1001',
        groupUid: 'GRP-01',
        guestName: 'Budi',
        guestDestination: 'Blok A',
      }),
    ];

    const useCase = new GetGuestsUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: { group_uid: null },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value).toHaveLength(0);
  });

  it('meneruskan filter isActive ke repository getGuests', async () => {
    const mockRepo = new MockGuestRepository();
    const getGuestsSpy = vi.spyOn(mockRepo, 'getGuests');
    const useCase = new GetGuestsUseCase(mockRepo);

    await useCase.execute({
      currentUser: { group_uid: 'GRP-01' },
      isActive: true,
    });

    expect(getGuestsSpy).toHaveBeenCalledWith(
      'GRP-01',
      expect.objectContaining({ isActive: true }),
    );
  });
});
