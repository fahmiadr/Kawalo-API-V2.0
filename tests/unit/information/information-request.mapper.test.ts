import { describe, expect, it } from 'vitest';
import { AppConstants } from '@core/constants/app.constants';
import type { AuthenticatedUserPayload } from '@core/types/jwt-payload.types';
import { InformationRequestMapper } from '@features/information/presentation/mappers/information-request.mapper';

describe('InformationRequestMapper Unit Tests', () => {
  const dummyUser: AuthenticatedUserPayload = {
    sub: 'USR-001',
    email: 'admin@kawalo.id',
    name: 'Admin Kawalo',
    role: 'ADMIN',
    group_uid: 'GRP-01',
    session_ver: 1,
  };

  describe('toGetInformationInput', () => {
    it('berhasil membersihkan query parameters ke GetInformationInput yang valid', () => {
      const query = {
        search: '  Pengumuman Libur  ',
        limit: 15,
        offset: 30,
        start_date: '2026-09-01T00:00:00.000Z',
        end_date: '2026-09-10T23:59:59.000Z',
        created_by: '  satpam@kawalo.id  ',
      };

      const input = InformationRequestMapper.toGetInformationInput(query, dummyUser);

      expect(input.search).toBe('Pengumuman Libur');
      expect(input.limit).toBe(15);
      expect(input.offset).toBe(30);
      expect(input.startDate).toBe('2026-09-01T00:00:00.000Z');
      expect(input.endDate).toBe('2026-09-10T23:59:59.000Z');
      expect(input.createdBy).toBe('satpam@kawalo.id');
      expect(input.currentUser).toBe(dummyUser);
    });

    it('mengonversi dirty strings pada query menjadi undefined', () => {
      const query = {
        search: '   ',
        start_date: 'null',
        end_date: 'N/A',
        created_by: '   ',
      };

      const input = InformationRequestMapper.toGetInformationInput(query, dummyUser);

      expect(input.search).toBeUndefined();
      expect(input.startDate).toBeUndefined();
      expect(input.endDate).toBeUndefined();
      expect(input.createdBy).toBeUndefined();
    });

    it('menyelesaikan paginasi default jika limit dan offset tidak diisi', () => {
      const input = InformationRequestMapper.toGetInformationInput({}, dummyUser);

      expect(input.limit).toBe(AppConstants.DEFAULT_PAGE_SIZE);
      expect(input.offset).toBe(0);
    });
  });

  describe('toCreateInformationInput', () => {
    it('berhasil membersihkan body CreateInformationDto', () => {
      const body = {
        title: '  Kerja Bakti Minggu Ini  ',
        text: '  Diharapkan semua warga berkumpul di posko  ',
        feed_type: 'EVENT' as const,
        feed_file: '  jadwal.pdf  ',
        photos: ['  https://cdn.kawalo.id/p1.jpg  ', 'N/A', '   ', 'https://cdn.kawalo.id/p2.jpg'],
        event_location: '  Pos RW 05  ',
        glat: '  -6.208800  ',
        glong: '  106.845600  ',
      };

      const input = InformationRequestMapper.toCreateInformationInput(body, dummyUser);

      expect(input.data.title).toBe('Kerja Bakti Minggu Ini');
      expect(input.data.text).toBe('Diharapkan semua warga berkumpul di posko');
      expect(input.data.feedType).toBe('EVENT');
      expect(input.data.feedFile).toBe('jadwal.pdf');
      expect(input.data.photos).toEqual([
        'https://cdn.kawalo.id/p1.jpg',
        'https://cdn.kawalo.id/p2.jpg',
      ]);
      expect(input.data.eventLocation).toBe('Pos RW 05');
      expect(input.data.glat).toBe('-6.208800');
      expect(input.data.glong).toBe('106.845600');
      expect(input.currentUser).toBe(dummyUser);
    });

    it('menangani input dirty strings dengan aman (mengembalikan undefined tanpa fallback)', () => {
      const body = {
        title: '   ',
        text: 'N/A',
        feed_type: 'null' as any,
        feed_file: null,
        photos: ['N/A', '   '],
        event_location: '   ',
        glat: '0.0',
        glong: 'N/A',
      };

      const input = InformationRequestMapper.toCreateInformationInput(body, dummyUser);

      expect(input.data.title).toBeUndefined();
      expect(input.data.text).toBeUndefined();
      expect(input.data.feedType).toBeUndefined();
      expect(input.data.feedFile).toBeUndefined();
      expect(input.data.photos).toBeUndefined();
      expect(input.data.eventLocation).toBeUndefined();
      expect(input.data.glat).toBeUndefined();
      expect(input.data.glong).toBeUndefined();
      expect(input.currentUser).toBe(dummyUser);
    });
  });

  describe('toUpdateInformationInput', () => {
    it('berhasil membersihkan partial updates pada body UpdateInformationDto', () => {
      const params = { id: '  501  ' };
      const body = {
        title: '  Judul Baru Terupdate  ',
      };

      const input = InformationRequestMapper.toUpdateInformationInput(params, body, dummyUser);

      expect(input.id).toBe('501');
      expect(input.data.title).toBe('Judul Baru Terupdate');
      expect(input.data.text).toBeUndefined();
      expect(input.data.photos).toBeUndefined();
    });

    it('membersihkan array photos saat diperbarui dan auto-slide ke kiri', () => {
      const params = { id: '501' };
      const body = {
        photos: [
          '',
          'https://cdn.kawalo.id/photo-1.jpg',
          '   ',
          'https://cdn.kawalo.id/photo-3.jpg',
        ],
      };

      const input = InformationRequestMapper.toUpdateInformationInput(params, body, dummyUser);

      // Indeks 1 dan 3 harus bergeser mengisi indeks 0 dan 1
      expect(input.data.photos).toEqual([
        'https://cdn.kawalo.id/photo-1.jpg',
        'https://cdn.kawalo.id/photo-3.jpg',
      ]);
    });

    it('membersihkan koordinat pada UpdateInformationDto dengan semantik tri-state', () => {
      const params = { id: '501' };
      const body = {
        glat: '  -6.208800  ',
        glong: '0.0',
      };

      const input = InformationRequestMapper.toUpdateInformationInput(params, body, dummyUser);

      expect(input.data.glat).toBe('-6.208800');
      expect(input.data.glong).toBeNull();
    });

    it('mengubah input null atau kosong pada update data menjadi null (explicit clear) untuk mengosongkan kolom', () => {
      const params = { id: '501' };
      const body = {
        text: null,
        feed_file: '   ',
        event_location: 'null',
        glat: null,
        glong: '0.0',
        photos: [],
      };

      const input = InformationRequestMapper.toUpdateInformationInput(params, body, dummyUser);

      expect(input.data.text).toBeNull();
      expect(input.data.feedFile).toBeNull();
      expect(input.data.eventLocation).toBeNull();
      expect(input.data.glat).toBeNull();
      expect(input.data.glong).toBeNull();
      expect(input.data.photos).toEqual([]);
      expect(input.data.title).toBeUndefined();
    });
  });


  describe('toDeleteInformationInput', () => {
    it('membersihkan ID string pada DeleteInformationInput', () => {
      const params = { id: '  501  ' };
      const input = InformationRequestMapper.toDeleteInformationInput(params, dummyUser);

      expect(input.id).toBe('501');
      expect(input.currentUser).toBe(dummyUser);
    });
  });

  describe('toDeleteBatchInformationInput', () => {
    it('membersihkan daftar ID string pada DeleteInformationInput', () => {
      const body = { ids: ['  501  ', '502', '   ', 'N/A'] };
      const input = InformationRequestMapper.toDeleteBatchInformationInput(body, dummyUser);

      expect(input.ids).toEqual(['501', '502']);
      expect(input.currentUser).toBe(dummyUser);
    });
  });

  describe('cleanAndSlidePhotos', () => {
    it('menggeser elemen valid ke kiri saat terdapat slot kosong di awal atau di antara elemen', () => {
      // index 0 kosong, index 1 ada, index 2 kosong, index 3 ada
      const input = ['', 'https://cdn.kawalo.id/1.jpg', '   ', 'https://cdn.kawalo.id/3.jpg'];
      const result = InformationRequestMapper.cleanAndSlidePhotos(input);

      // Hasil harus tergeser: index 1 -> 0, index 3 -> 1
      expect(result).toEqual(['https://cdn.kawalo.id/1.jpg', 'https://cdn.kawalo.id/3.jpg']);
    });

    it('membatasi maksimal 4 foto tergeser', () => {
      const input = [
        'https://cdn.kawalo.id/1.jpg',
        'https://cdn.kawalo.id/2.jpg',
        'https://cdn.kawalo.id/3.jpg',
        'https://cdn.kawalo.id/4.jpg',
        'https://cdn.kawalo.id/5.jpg',
      ];
      const result = InformationRequestMapper.cleanAndSlidePhotos(input);

      expect(result).toEqual([
        'https://cdn.kawalo.id/1.jpg',
        'https://cdn.kawalo.id/2.jpg',
        'https://cdn.kawalo.id/3.jpg',
        'https://cdn.kawalo.id/4.jpg',
      ]);
    });

    it('mengembalikan undefined jika parameter undefined dan array kosong jika seluruh item dirty atau kosong', () => {
      expect(InformationRequestMapper.cleanAndSlidePhotos(undefined)).toBeUndefined();
      expect(InformationRequestMapper.cleanAndSlidePhotos(['  ', 'N/A', 'null'])).toEqual([]);
      expect(InformationRequestMapper.cleanAndSlidePhotos([])).toEqual([]);
      expect(InformationRequestMapper.cleanAndSlidePhotos(null)).toEqual([]);
    });
  });
});

