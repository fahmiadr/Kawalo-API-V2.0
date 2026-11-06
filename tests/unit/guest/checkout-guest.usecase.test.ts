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
import { CheckoutGuestUseCase } from '@features/guest/usecases/checkout-guest.usecase';
import { describe, expect, it, vi } from 'vitest';

class MockGuestRepository implements GuestRepository {
  public createGuestFn = vi.fn();
  public getGuestsFn = vi.fn();
  public getGuestByIdFn = vi.fn();
  public updateGuestFn = vi.fn();
  public isValidGuestTypeFn = vi.fn();
  public getMasterGuestTypesFn = vi.fn();
  public getMasterHostsFn = vi.fn();

  async getGuests(groupUid: string, filter?: GuestFilterParams): Promise<GuestEntity[]> {
    return this.getGuestsFn(groupUid, filter);
  }

  async getGuestById(id: string, groupUid: string): Promise<GuestEntity | null> {
    return this.getGuestByIdFn(id, groupUid);
  }

  async createGuest(params: CreateGuestParams): Promise<GuestEntity> {
    return this.createGuestFn(params);
  }

  async updateGuest(params: UpdateGuestParams): Promise<GuestEntity> {
    return this.updateGuestFn(params);
  }

  public getGuestsByIdsFn = vi.fn();
  async getGuestsByIds(ids: string[], groupUid: string): Promise<GuestEntity[]> {
    return this.getGuestsByIdsFn(ids, groupUid);
  }

  public deleteManyGuestsFn = vi.fn();
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

describe('CheckoutGuestUseCase Unit Tests', () => {
  const activeGuest = new GuestEntity({
    id: '1',
    guestUid: 'GST-001',
    groupUid: 'GRP-01',
    guestName: 'Pak Budi',
    guestDestination: 'Blok B2',
    guestToUserEmail: 'resident@veranda.com',
    guestCreatorEmail: 'guard1@veranda.com',
    guestDt: '2026-09-21T08:00:00.000Z',
    guestAckDt: '2026-09-21T08:05:00.000Z',
    guestOutDt: null,
    guestCheckOutByEmail: null,
  });

  it('ADMIN dapat melakukan checkout tamu dalam grup aktif', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(activeGuest);
    mockRepo.updateGuestFn.mockResolvedValue(
      new GuestEntity({
        ...activeGuest,
        guestOutDt: '2026-09-21T10:00:00.000Z',
        guestCheckOutByEmail: 'admin@veranda.com',
      }),
    );

    const useCase = new CheckoutGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '1',
      currentUser: {
        email: 'admin@veranda.com',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value.guestOutDt).toBeDefined();
    expect(result.value.guestCheckOutByEmail).toBe('admin@veranda.com');
    expect(mockRepo.updateGuestFn).toHaveBeenCalledWith(
      expect.objectContaining({
        context: expect.objectContaining({
          id: '1',
          groupUid: 'GRP-01',
          checkOutByEmail: 'admin@veranda.com',
        }),
        system: expect.objectContaining({
          guestOutDt: expect.any(Date),
        }),
      }),
    );
  });

  it('GUARD dapat melakukan checkout tamu dalam grup aktif', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(activeGuest);
    mockRepo.updateGuestFn.mockResolvedValue(
      new GuestEntity({
        ...activeGuest,
        guestOutDt: '2026-09-21T10:00:00.000Z',
        guestCheckOutByEmail: 'guard2@veranda.com',
      }),
    );

    const useCase = new CheckoutGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '1',
      currentUser: {
        email: 'guard2@veranda.com',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(mockRepo.updateGuestFn).toHaveBeenCalledWith(
      expect.objectContaining({
        context: expect.objectContaining({
          id: '1',
          groupUid: 'GRP-01',
          checkOutByEmail: 'guard2@veranda.com',
        }),
        system: expect.objectContaining({
          guestOutDt: expect.any(Date),
        }),
      }),
    );
  });

  it('Gagal jika tamu sudah pernah checkout sebelumnya', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(
      new GuestEntity({
        ...activeGuest,
        guestOutDt: '2026-09-21T09:30:00.000Z',
        guestCheckOutByEmail: 'guard1@veranda.com',
      }),
    );

    const useCase = new CheckoutGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '1',
      currentUser: {
        email: 'guard1@veranda.com',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
    expect(result.failure.message).toContain('sudah keluar/selesai');
    expect(mockRepo.updateGuestFn).not.toHaveBeenCalled();
  });

  it('checkout langsung diizinkan meskipun belum pernah di-ack (guestAckDt null)', async () => {
    const unacknowledgedGuest = new GuestEntity({
      ...activeGuest,
      guestAckDt: null,
    });

    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(unacknowledgedGuest);
    mockRepo.updateGuestFn.mockResolvedValue(
      new GuestEntity({
        ...unacknowledgedGuest,
        guestOutDt: '2026-09-21T10:00:00.000Z',
        guestCheckOutByEmail: 'guard1@veranda.com',
      }),
    );

    const useCase = new CheckoutGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '1',
      currentUser: {
        email: 'guard1@veranda.com',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value.guestOutDt).toBeDefined();
    expect(mockRepo.updateGuestFn).toHaveBeenCalled();
  });

  it('Berhasil checkout menggunakan qrUid', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(activeGuest);
    mockRepo.updateGuestFn.mockResolvedValue(
      new GuestEntity({
        ...activeGuest,
        guestOutDt: '2026-09-21T10:00:00.000Z',
        guestCheckOutByEmail: 'guard1@veranda.com',
      }),
    );

    const useCase = new CheckoutGuestUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: {
        email: 'guard1@veranda.com',
        group_uid: 'GRP-01',
      },
      data: {
        qrUid: 'QR-VER-001',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(mockRepo.getGuestByIdFn).toHaveBeenCalledWith('QR-VER-001', 'GRP-01');
    expect(mockRepo.updateGuestFn).toHaveBeenCalledWith(
      expect.objectContaining({
        context: expect.objectContaining({
          id: '1', // Selalu menggunakan numeric ID asli
          groupUid: 'GRP-01',
        }),
      }),
    );
  });
});
