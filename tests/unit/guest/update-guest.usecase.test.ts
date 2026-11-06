import { ValidationFailure } from '@core/error/failure';
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
import { UpdateGuestUseCase } from '@features/guest/usecases/update-guest.usecase';
import { describe, expect, it, vi } from 'vitest';

class MockGuestRepository implements GuestRepository {
  public createGuestFn = vi.fn();
  public getGuestsFn = vi.fn();
  public getGuestByIdFn = vi.fn();
  public updateGuestFn = vi.fn();
  public isValidGuestTypeFn = vi.fn();
  public resolveCanonicalGuestTypeFn = vi.fn();
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

  async resolveCanonicalGuestType(guestType: string, groupUid: string): Promise<string | null> {
    return this.resolveCanonicalGuestTypeFn(guestType, groupUid);
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

describe('UpdateGuestUseCase Unit Tests', () => {
  const existingGuestFromGuard = new GuestEntity({
    id: '10',
    guestUid: 'GST-010',
    groupUid: 'GRP-01',
    guestName: 'Tamu Guard',
    guestDestination: 'Blok C1',
    guestType: 'TAMU',
    guestCreatorEmail: 'guard1@veranda.com',
    guestCreatorName: 'Satpam Satu',
  });

  const existingGuestFromOther = new GuestEntity({
    id: '20',
    guestUid: 'GST-020',
    groupUid: 'GRP-01',
    guestName: 'Tamu Orang Lain',
    guestDestination: 'Blok D2',
    guestType: 'TAMU',
    guestCreatorEmail: 'someoneelse@veranda.com',
    guestCreatorName: 'Orang Lain',
  });

  it('ADMIN dapat mengubah data tamu mana pun dalam grup aktif', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(existingGuestFromOther);
    mockRepo.updateGuestFn.mockResolvedValue(
      new GuestEntity({
        ...existingGuestFromOther,
        guestName: 'Tamu Diperbarui Admin',
      }),
    );

    const useCase = new UpdateGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '20',
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
      data: {
        guestName: 'Tamu Diperbarui Admin',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value.guestName).toBe('Tamu Diperbarui Admin');
    expect(mockRepo.updateGuestFn).toHaveBeenCalledWith(
      expect.objectContaining({
        context: expect.objectContaining({
          id: '20',
          groupUid: 'GRP-01',
        }),
        data: expect.objectContaining({ guestName: 'Tamu Diperbarui Admin' }),
      }),
    );
  });

  it('GUARD dapat mengubah data tamu miliknya sendiri', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(existingGuestFromGuard);
    mockRepo.updateGuestFn.mockResolvedValue(
      new GuestEntity({
        ...existingGuestFromGuard,
        guestName: 'Tamu Diperbarui Guard Sendiri',
      }),
    );

    const useCase = new UpdateGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '10',
      currentUser: {
        email: 'guard1@veranda.com',
        role: 'GUARD',
        group_uid: 'GRP-01',
      },
      data: {
        guestName: 'Tamu Diperbarui Guard Sendiri',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value.guestName).toBe('Tamu Diperbarui Guard Sendiri');
    expect(mockRepo.updateGuestFn).toHaveBeenCalledWith(
      expect.objectContaining({
        context: expect.objectContaining({
          id: '10',
          groupUid: 'GRP-01',
        }),
        data: expect.objectContaining({ guestName: 'Tamu Diperbarui Guard Sendiri' }),
      }),
    );
  });

  it('berhasil update dan mengkanonisasi casing guestType jika valid di grup aktif', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(existingGuestFromGuard);
    mockRepo.resolveCanonicalGuestTypeFn.mockResolvedValue('KONTRAKTOR');
    mockRepo.updateGuestFn.mockResolvedValue(
      new GuestEntity({
        ...existingGuestFromGuard,
        guestType: 'KONTRAKTOR',
      }),
    );

    const useCase = new UpdateGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '10',
      currentUser: {
        email: 'guard1@veranda.com',
        role: 'GUARD',
        group_uid: 'GRP-01',
      },
      data: {
        guestType: 'kontraktor', // lowercase input dari client
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(mockRepo.resolveCanonicalGuestTypeFn).toHaveBeenCalledWith('kontraktor', 'GRP-01');
    expect(mockRepo.updateGuestFn).toHaveBeenCalledWith(
      expect.objectContaining({
        context: expect.objectContaining({
          id: '10',
          groupUid: 'GRP-01',
        }),
        data: expect.objectContaining({ guestType: 'KONTRAKTOR' }), // tersimpan canonical uppercase
      }),
    );
    expect(result.value.guestType).toBe('KONTRAKTOR');
  });

  it('gagal update jika guestType baru tidak valid di grup aktif', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(existingGuestFromGuard);
    mockRepo.resolveCanonicalGuestTypeFn.mockResolvedValue(null);

    const useCase = new UpdateGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '10',
      currentUser: {
        email: 'guard1@veranda.com',
        role: 'GUARD',
        group_uid: 'GRP-01',
      },
      data: {
        guestType: 'TIDAK_VALID',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
    expect(result.failure.message).toContain('Tipe tamu tidak valid');
    expect(mockRepo.updateGuestFn).not.toHaveBeenCalled();
  });

  it('GUARD ditolak (403 Forbidden) jika mencoba mengubah data tamu milik orang lain', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(existingGuestFromOther);

    const useCase = new UpdateGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '20',
      currentUser: {
        email: 'guard1@veranda.com',
        role: 'GUARD',
        group_uid: 'GRP-01',
      },
      data: {
        guestName: 'Coba Edit',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Forbidden');
    expect(mockRepo.updateGuestFn).not.toHaveBeenCalled();
  });

  it('Mengembalikan 404 Not Found jika data tamu tidak ditemukan', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(null);

    const useCase = new UpdateGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '999',
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
      data: { guestName: 'Tak Ada' },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Not Found');
    expect(mockRepo.updateGuestFn).not.toHaveBeenCalled();
  });

  it('berhasil memperbarui guestIdentityType jika disediakan', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(existingGuestFromOther);
    mockRepo.updateGuestFn.mockResolvedValue(
      new GuestEntity({
        ...existingGuestFromOther,
        guestIdentity: 'SIM123',
        guestIdentityType: 'SIM',
      }),
    );

    const useCase = new UpdateGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '20',
      currentUser: {
        email: 'admin@veranda.com',
        role: 'ADMIN',
        group_uid: 'GRP-01',
      },
      data: {
        guestIdentity: 'SIM123',
        guestIdentityType: 'SIM',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(mockRepo.updateGuestFn).toHaveBeenCalledWith(
      expect.objectContaining({
        data: expect.objectContaining({
          guestIdentity: 'SIM123',
          guestIdentityType: 'SIM',
        }),
      }),
    );
  });

  it('gagal jika guestToUserEmail yang diupdate tidak terdaftar pada grup aktif', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(existingGuestFromGuard);
    mockRepo.isValidHostFn.mockResolvedValue(false);

    const useCase = new UpdateGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '10',
      currentUser: {
        email: 'guard1@veranda.com',
        role: 'GUARD',
        group_uid: 'GRP-01',
      },
      data: {
        guestToUserEmail: 'notfound@veranda.com',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure).toBeInstanceOf(ValidationFailure);
    expect(result.failure.message).toBe('Penghuni yang dituju tidak terdaftar pada grup aktif.');
    expect(mockRepo.updateGuestFn).not.toHaveBeenCalled();
  });

  it('berhasil meneruskan nilai null untuk mengosongkan guestPurpose, guestNumber, guestIdentity, guestAddress, guestToUserEmail, dan guestPhoto', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.getGuestByIdFn.mockResolvedValue(existingGuestFromGuard);
    mockRepo.updateGuestFn.mockResolvedValue(
      new GuestEntity({
        ...existingGuestFromGuard,
        guestPurpose: null,
        guestNumber: null,
        guestIdentity: null,
        guestIdentityType: null,
        guestAddress: null,
        guestToUserEmail: null,
        guestPhoto: null,
      }),
    );

    const useCase = new UpdateGuestUseCase(mockRepo);
    const result = await useCase.execute({
      id: '10',
      currentUser: {
        email: 'guard1@veranda.com',
        role: 'GUARD',
        group_uid: 'GRP-01',
      },
      data: {
        guestPurpose: null,
        guestNumber: null,
        guestIdentity: null,
        guestIdentityType: null,
        guestAddress: null,
        guestToUserEmail: null,
        guestPhoto: null,
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(mockRepo.updateGuestFn).toHaveBeenCalledWith(
      expect.objectContaining({
        context: {
          id: '10',
          groupUid: 'GRP-01',
        },
        data: expect.objectContaining({
          guestPurpose: null,
          guestNumber: null,
          guestIdentity: null,
          guestIdentityType: null,
          guestAddress: null,
          guestToUserEmail: null,
          guestPhoto: null,
        }),
      }),
    );
  });
});

