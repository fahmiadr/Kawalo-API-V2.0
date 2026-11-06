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
import { CreateGuestUseCase } from '@features/guest/usecases/create-guest.usecase';
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

describe('CreateGuestUseCase Unit Tests', () => {
  it('berhasil membuat tamu jika role adalah ADMIN dan mengkanonisasi casing guestType', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.resolveCanonicalGuestTypeFn.mockResolvedValue('TAMU UMUM');
    mockRepo.createGuestFn.mockResolvedValue(
      new GuestEntity({
        id: '1',
        guestUid: 'GST-001',
        groupUid: 'GRP-01',
        guestName: 'Pak Budi',
        guestDestination: 'Blok B2',
        guestType: 'TAMU UMUM',
        guestCreatorEmail: 'admin@veranda.com',
        guestCreatorName: 'Admin Veranda',
      }),
    );

    const useCase = new CreateGuestUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: {
        email: 'admin@veranda.com',
        name: 'Admin Veranda',
        group_uid: 'GRP-01',
      },
      data: {
        guestName: 'Pak Budi',
        guestDestination: 'Blok B2',
        guestType: 'tamu umum', // lowercase input dari client
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value.id).toBe('1');
    expect(result.value.guestName).toBe('Pak Budi');
    expect(result.value.guestType).toBe('TAMU UMUM');
    expect(mockRepo.resolveCanonicalGuestTypeFn).toHaveBeenCalledWith('tamu umum', 'GRP-01');
    expect(mockRepo.createGuestFn).toHaveBeenCalledWith(
      expect.objectContaining({
        system: expect.objectContaining({
          guestUid: expect.stringMatching(/^guest_[0-9a-f-]{36}$/),
          qrUid: expect.stringMatching(
            /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
          ),
          guestCategory: 'Umum',
          guestDt: expect.any(Date),
        }),
        context: expect.objectContaining({
          groupUid: 'GRP-01',
          creatorEmail: 'admin@veranda.com',
          creatorName: 'Admin Veranda',
        }),
        data: expect.objectContaining({
          guestName: 'Pak Budi',
          guestDestination: 'Blok B2',
          guestType: 'TAMU UMUM', // tersimpan dalam canonical uppercase dari DB master
        }),
      }),
    );
  });

  it('berhasil membuat tamu jika role adalah GUARD dan guestType valid', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.resolveCanonicalGuestTypeFn.mockResolvedValue('KURIR');
    mockRepo.createGuestFn.mockResolvedValue(
      new GuestEntity({
        id: '2',
        guestUid: 'GST-002',
        groupUid: 'GRP-01',
        guestName: 'Kurir Paket',
        guestDestination: 'Pos Satpam',
        guestType: 'KURIR',
        guestCreatorEmail: 'guard@veranda.com',
        guestCreatorName: 'Petugas Jaga',
      }),
    );

    const useCase = new CreateGuestUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: {
        email: 'guard@veranda.com',
        name: 'Petugas Jaga',
        group_uid: 'GRP-01',
      },
      data: {
        guestName: 'Kurir Paket',
        guestDestination: 'Pos Satpam',
        guestType: 'KURIR',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(result.value.id).toBe('2');
    expect(mockRepo.createGuestFn).toHaveBeenCalledWith(
      expect.objectContaining({
        system: expect.objectContaining({
          guestUid: expect.stringMatching(/^guest_[0-9a-f-]{36}$/),
          qrUid: expect.stringMatching(
            /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
          ),
          guestCategory: 'Umum',
          guestDt: expect.any(Date),
        }),
        context: expect.objectContaining({
          groupUid: 'GRP-01',
          creatorEmail: 'guard@veranda.com',
          creatorName: 'Petugas Jaga',
        }),
        data: expect.objectContaining({
          guestName: 'Kurir Paket',
          guestDestination: 'Pos Satpam',
          guestType: 'KURIR',
        }),
      }),
    );
  });

  it('gagal jika guestType tidak terdaftar di tkmasterguesttype untuk grup aktif', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.resolveCanonicalGuestTypeFn.mockResolvedValue(null);

    const useCase = new CreateGuestUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: {
        email: 'guard@veranda.com',
        name: 'Petugas Jaga',
        group_uid: 'GRP-01',
      },
      data: {
        guestName: 'Orang Asing',
        guestDestination: 'Blok C',
        guestType: 'TIDAK_TERDAFTAR',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
    expect(result.failure.message).toContain('Tipe tamu tidak valid atau tidak terdaftar');
    expect(mockRepo.createGuestFn).not.toHaveBeenCalled();
  });

  it('gagal jika guestType kosong atau tidak disediakan', async () => {
    const mockRepo = new MockGuestRepository();

    const useCase = new CreateGuestUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: {
        email: 'guard@veranda.com',
        name: 'Petugas Jaga',
        group_uid: 'GRP-01',
      },
      data: {
        guestName: 'Orang Asing',
        guestDestination: 'Blok C',
        guestType: '',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
    expect(result.failure.message).toContain('Tipe tamu wajib diisi');
    expect(mockRepo.createGuestFn).not.toHaveBeenCalled();
  });

  it('gagal (Bad Request) jika group_uid belum ada', async () => {
    const mockRepo = new MockGuestRepository();
    const useCase = new CreateGuestUseCase(mockRepo);

    const result = await useCase.execute({
      currentUser: {
        email: 'admin@veranda.com',
        name: 'Admin',
        group_uid: null,
      },
      data: {
        guestName: 'Tamu',
        guestDestination: 'Blok A1',
        guestType: 'TAMU',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
  });

  it('gagal (Bad Request) jika data personil (email/name) tidak valid (anti-poisoning)', async () => {
    const mockRepo = new MockGuestRepository();
    const useCase = new CreateGuestUseCase(mockRepo);

    const result = await useCase.execute({
      currentUser: {
        email: '',
        name: 'Admin',
        group_uid: 'GRP-01',
      },
      data: {
        guestName: 'Tamu',
        guestDestination: 'Blok A1',
        guestType: 'TAMU',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure.status).toBe('Bad Request');
    expect(result.failure.message).toContain('Data personil (email dan nama) tidak valid.');
  });

  it('menggunakan default guestCategory "Umum" dan server timestamp pada repository.createGuest', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.resolveCanonicalGuestTypeFn.mockResolvedValue('TAMU');
    mockRepo.createGuestFn.mockResolvedValue(
      new GuestEntity({
        id: '3',
        guestUid: 'GST-003',
        groupUid: 'GRP-01',
        guestName: 'Tamu Default',
        guestDestination: 'Blok D',
        guestType: 'TAMU',
        guestCategory: 'Umum',
      }),
    );

    const useCase = new CreateGuestUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: {
        email: 'guard@veranda.com',
        name: 'Guard',
        group_uid: 'GRP-01',
      },
      data: {
        guestName: 'Tamu Default',
        guestDestination: 'Blok D',
        guestType: 'TAMU',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(mockRepo.createGuestFn).toHaveBeenCalledWith(
      expect.objectContaining({
        system: expect.objectContaining({
          guestCategory: 'Umum',
          guestDt: expect.any(Date),
        }),
      }),
    );
  });

  it('berhasil menyimpan data tamu beserta guestIdentityType', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.resolveCanonicalGuestTypeFn.mockResolvedValue('TAMU');
    mockRepo.createGuestFn.mockResolvedValue(
      new GuestEntity({
        id: '4',
        guestUid: 'GST-004',
        groupUid: 'GRP-01',
        guestName: 'Tamu Berpaspor',
        guestDestination: 'Blok E1',
        guestType: 'TAMU',
        guestIdentity: 'A12345678',
        guestIdentityType: 'PASPOR',
      }),
    );

    const useCase = new CreateGuestUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: {
        email: 'guard@veranda.com',
        name: 'Guard',
        group_uid: 'GRP-01',
      },
      data: {
        guestName: 'Tamu Berpaspor',
        guestDestination: 'Blok E1',
        guestType: 'TAMU',
        guestIdentity: 'A12345678',
        guestIdentityType: 'PASPOR',
      },
    });

    expect(result.isSuccess()).toBe(true);
    expect(mockRepo.createGuestFn).toHaveBeenCalledWith(
      expect.objectContaining({
        system: expect.objectContaining({
          qrUid: expect.stringMatching(
            /^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
          ),
        }),
        data: expect.objectContaining({
          guestIdentity: 'A12345678',
          guestIdentityType: 'PASPOR',
        }),
      }),
    );
  });

  it('gagal jika guestToUserEmail tidak terdaftar pada grup aktif', async () => {
    const mockRepo = new MockGuestRepository();
    mockRepo.resolveCanonicalGuestTypeFn.mockResolvedValue('TAMU');
    mockRepo.isValidHostFn.mockResolvedValue(false);

    const useCase = new CreateGuestUseCase(mockRepo);
    const result = await useCase.execute({
      currentUser: {
        email: 'guard@veranda.com',
        name: 'Guard',
        group_uid: 'GRP-01',
      },
      data: {
        guestName: 'Tamu Budi',
        guestDestination: 'Blok A1',
        guestType: 'TAMU',
        guestToUserEmail: 'notfound@veranda.com',
      },
    });

    expect(result.isFailure()).toBe(true);
    expect(result.failure).toBeInstanceOf(ValidationFailure);
    expect(result.failure.message).toBe('Penghuni yang dituju tidak terdaftar pada grup aktif.');
    expect(mockRepo.createGuestFn).not.toHaveBeenCalled();
  });
});
