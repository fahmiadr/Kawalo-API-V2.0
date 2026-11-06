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
import { AcknowledgeGuestUseCase } from '@features/guest/usecases/acknowledge-guest.usecase';
import { describe, expect, it, vi } from 'vitest';

class MockGuestRepository implements GuestRepository {
  public createGuestFn = vi.fn();
  public getGuestsFn = vi.fn();
  public getGuestByIdFn = vi.fn();
  public updateGuestFn = vi.fn();
  public isValidGuestTypeFn = vi.fn();
  public getMasterGuestTypesFn = vi.fn();

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

  public getMasterHostsFn = vi.fn();

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

describe('AcknowledgeGuestUseCase Unit Tests', () => {
  const pendingGuest = new GuestEntity({
    id: '1',
    guestUid: 'GST-001',
    groupUid: 'GRP-01',
    guestName: 'Pak Budi',
    guestDestination: 'Blok B2',
    guestToUserEmail: 'resident@veranda.com',
    guestCreatorEmail: 'guard@veranda.com',
    guestDt: '2026-09-21T08:00:00.000Z',
    guestAckDt: null,
    guestOutDt: null,
  });

  it('ADMIN dapat mengonfirmasi tamu dalam grup aktif', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(pendingGuest);
    mockRepo.updateGuestFn.mockResolvedValue(
      new GuestEntity({
        ...pendingGuest,
        guestAckDt: '2026-09-21T08:05:00.000Z',
      }),
    );

    const useCase = new AcknowledgeGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '1',
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value.guestAckDt).toBe('2026-09-21T08:05:00.000Z');
    expect(mockRepo.updateGuestFn).toHaveBeenCalledWith(
      expect.objectContaining({
        context: expect.objectContaining({
          id: '1',
          groupUid: 'GRP-01',
        }),
        system: expect.objectContaining({ guestAckDt: expect.any(Date) }),
      }),
    );
  });

  it('GUARD dapat mengonfirmasi tamu dalam grup aktif', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(pendingGuest);
    mockRepo.updateGuestFn.mockResolvedValue(
      new GuestEntity({
        ...pendingGuest,
        guestAckDt: '2026-09-21T08:05:00.000Z',
      }),
    );

    const useCase = new AcknowledgeGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '1',
      currentUser: {
        email: 'guard2@veranda.com',
        role: 'GUARD',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(mockRepo.updateGuestFn).toHaveBeenCalled();
  });

  it('Penghuni yang dituju (host) dapat mengonfirmasi tamu', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(pendingGuest);
    mockRepo.updateGuestFn.mockResolvedValue(
      new GuestEntity({
        ...pendingGuest,
        guestAckDt: '2026-09-21T08:05:00.000Z',
      }),
    );

    const useCase = new AcknowledgeGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '1',
      currentUser: {
        email: 'resident@veranda.com',
        role: 'USER',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(mockRepo.updateGuestFn).toHaveBeenCalled();
  });

  it('Penghuni lain yang bukan host ditolak (403 Forbidden)', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(pendingGuest);

    const useCase = new AcknowledgeGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '1',
      currentUser: {
        email: 'otherresident@veranda.com',
        role: 'USER',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Forbidden');
    expect(mockRepo.updateGuestFn).not.toHaveBeenCalled();
  });

  it('Gagal jika tamu sudah keluar (sudah check-out)', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(
      new GuestEntity({
        ...pendingGuest,
        guestOutDt: '2026-09-21T09:00:00.000Z',
        guestCheckOutByEmail: 'guard@veranda.com',
      }),
    );

    const useCase = new AcknowledgeGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '1',
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
    expect(result.failure.message).toContain('sudah keluar');
    expect(mockRepo.updateGuestFn).not.toHaveBeenCalled();
  });

  it('Gagal jika data tamu tidak ditemukan (404 Not Found)', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(null);

    const useCase = new AcknowledgeGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '99',
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Not Found');
    expect(mockRepo.updateGuestFn).not.toHaveBeenCalled();
  });

  it('Gagal jika tamu sudah dikonfirmasi sebelumnya (tidak bisa ack dua kali)', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(
      new GuestEntity({
        ...pendingGuest,
        guestAckDt: '2026-09-21T08:05:00.000Z',
      }),
    );

    const useCase = new AcknowledgeGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '1',
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
    expect(result.failure.message).toContain('sudah dikonfirmasi sebelumnya');
    expect(mockRepo.updateGuestFn).not.toHaveBeenCalled();
  });

  it('Berhasil mengonfirmasi tamu menggunakan qrUid', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(pendingGuest);
    mockRepo.updateGuestFn.mockResolvedValue(
      new GuestEntity({
        ...pendingGuest,
        guestAckDt: '2026-09-21T08:05:00.000Z',
      }),
    );

    const useCase = new AcknowledgeGuestUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: {
        email: 'guard@veranda.com',
        role: 'GUARD',
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
          id: '1', // Entitas asli ID numerik
          groupUid: 'GRP-01',
        }),
      }),
    );
  });
});
