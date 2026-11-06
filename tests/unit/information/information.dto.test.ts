import {
  CreateInformationDto,
  DeleteBatchInformationDto,
  GetInformationQueryDto,
  InformationIdParamsDto,
  UpdateInformationDto,
} from '@features/information/presentation/dtos/information-request.dto';
import {
  DeleteBatchInformationResponseDto,
  GetInformationResponseDto,
  InformationItemDto,
} from '@features/information/presentation/dtos/information-response.dto';
import { Value } from '@sinclair/typebox/value';
import { describe, expect, it } from 'vitest';

describe('Information DTO Schema Validation Unit Tests', () => {
  describe('InformationIdParamsDto', () => {
    it('menerima ID numerik positif yang valid', () => {
      expect(Value.Check(InformationIdParamsDto, { id: '123' })).toBe(true);
      expect(Value.Check(InformationIdParamsDto, { id: '1' })).toBe(true);
    });

    it('menolak ID non-numerik atau string acak', () => {
      expect(Value.Check(InformationIdParamsDto, { id: 'abc' })).toBe(false);
      expect(Value.Check(InformationIdParamsDto, { id: '12-34' })).toBe(false);
      expect(Value.Check(InformationIdParamsDto, { id: '' })).toBe(false);
    });
  });

  describe('GetInformationQueryDto', () => {
    it('menerima query kosong dengan default parameter', () => {
      expect(Value.Check(GetInformationQueryDto, {})).toBe(true);
    });

    it('menerima filter pencarian, rentang tanggal, limit, offset, dan created_by valid', () => {
      const validQuery = {
        search: 'Pos Satpam',
        limit: 10,
        offset: 0,
        start_date: '2026-09-01T00:00:00.000Z',
        end_date: '2026-09-30T23:59:59.000Z',
        created_by: 'satpam@kawalo.id',
      };
      expect(Value.Check(GetInformationQueryDto, validQuery)).toBe(true);
    });
  });

  describe('CreateInformationDto', () => {
    it('menerima payload pembuatan informasi yang valid', () => {
      const validPayload = {
        title: 'Pengumuman Penting',
        text: 'Besok ada kegiatan gotong royong.',
        feed_type: 'EVENT',
        photos: ['https://cdn.kawalo.id/photo1.jpg'],
        event_location: 'Posko 1',
        glat: '-6.2088',
        glong: '106.8456',
      };
      expect(Value.Check(CreateInformationDto, validPayload)).toBe(true);
    });

    it('menolak payload tanpa title atau title string kosong', () => {
      expect(Value.Check(CreateInformationDto, { title: '' })).toBe(false);
      expect(Value.Check(CreateInformationDto, {})).toBe(false);
    });

    it('menolak title yang melebihi batas 128 karakter (VarChar 128)', () => {
      const longTitle = 'a'.repeat(129);
      expect(Value.Check(CreateInformationDto, { title: longTitle, feed_type: 'EVENT' })).toBe(
        false,
      );
    });

    it('menerima feed_type hanya dari enum [GENERAL, JOIN, INCIDENT, EVENT, OTHER]', () => {
      expect(
        Value.Check(CreateInformationDto, { title: 'Valid Title', feed_type: 'GENERAL' }),
      ).toBe(true);
      expect(Value.Check(CreateInformationDto, { title: 'Valid Title', feed_type: 'JOIN' })).toBe(
        true,
      );
      expect(
        Value.Check(CreateInformationDto, { title: 'Valid Title', feed_type: 'INCIDENT' }),
      ).toBe(true);
      expect(Value.Check(CreateInformationDto, { title: 'Valid Title', feed_type: 'EVENT' })).toBe(
        true,
      );
      expect(Value.Check(CreateInformationDto, { title: 'Valid Title', feed_type: 'OTHER' })).toBe(
        true,
      );
      expect(Value.Check(CreateInformationDto, { title: 'Valid Title' })).toBe(true); // default OTHER
    });

    it('menolak feed_type di luar enum yang ditentukan', () => {
      expect(Value.Check(CreateInformationDto, { title: 'Valid Title', feed_type: 'INFO' })).toBe(
        false,
      );
      expect(
        Value.Check(CreateInformationDto, {
          title: 'Valid Title',
          feed_type: 'COMMUNITY_ANNOUNCEMENT',
        }),
      ).toBe(false);
      expect(Value.Check(CreateInformationDto, { title: 'Valid Title', feed_type: '' })).toBe(
        false,
      );
    });

    it('mendukung URL foto dan lampiran panjang hingga 2048 karakter serta lokasi hingga 128 karakter', () => {
      const longUrl = `https://storage.googleapis.com/cdn/photos/${'x'.repeat(1900)}.jpg`;
      const longLocation = 'a'.repeat(128);
      const validPayload = {
        title: 'Pengumuman Baru',
        feed_type: 'EVENT',
        photos: [longUrl],
        feed_file: longUrl,
        event_location: longLocation,
      };
      expect(Value.Check(CreateInformationDto, validPayload)).toBe(true);

      const tooLongUrl = `https://${'x'.repeat(2048)}.jpg`;
      expect(
        Value.Check(CreateInformationDto, {
          ...validPayload,
          photos: [tooLongUrl],
        }),
      ).toBe(false);
    });

    it('menolak jika photos melebihi 4 item (kapasitas kolom FeedPic 1-4 di DB)', () => {
      const validPayload = {
        title: 'Pengumuman Baru',
        feed_type: 'EVENT',
        photos: [
          'https://cdn.kawalo.id/1.jpg',
          'https://cdn.kawalo.id/2.jpg',
          'https://cdn.kawalo.id/3.jpg',
          'https://cdn.kawalo.id/4.jpg',
        ],
      };
      expect(Value.Check(CreateInformationDto, validPayload)).toBe(true);

      expect(
        Value.Check(CreateInformationDto, {
          ...validPayload,
          photos: [...validPayload.photos, 'https://cdn.kawalo.id/5.jpg'],
        }),
      ).toBe(false);
    });
  });

  describe('UpdateInformationDto', () => {
    it('menerima update parsial dengan title baru', () => {
      expect(Value.Check(UpdateInformationDto, { title: 'Judul Baru' })).toBe(true);
    });

    it('menerima update tanpa field apa pun (empty object)', () => {
      expect(Value.Check(UpdateInformationDto, {})).toBe(true);
    });

    it('menolak update jika feed_type tidak valid', () => {
      expect(Value.Check(UpdateInformationDto, { feed_type: 'INVALID' })).toBe(false);
      expect(Value.Check(UpdateInformationDto, { feed_type: 'INCIDENT' })).toBe(true);
    });
  });

  describe('InformationItemDto & GetInformationResponseDto', () => {
    it('memvalidasi struktur InformationItemDto yang lengkap', () => {
      const item = {
        id: '101',
        title: 'Info Terkini',
        text: 'Isi pengumuman',
        user_email: 'admin@kawalo.id',
        user_name: 'Admin Kawalo',
        user_profile_pic: null,
        feed_dt: '2026-09-23T10:00:00.000Z',
        feed_file: null,
        photos: ['https://cdn.kawalo.id/p1.jpg'],
        feed_type: 'INFO',
        group_name: 'Cluster Sakura',
        event_location: 'Pos Gerbang',
        event_start_dt: null,
        event_end_dt: null,
        event_duration: null,
        glat: '-6.2000',
        glong: '106.8000',
      };

      expect(Value.Check(InformationItemDto, item)).toBe(true);
      expect(Value.Check(GetInformationResponseDto, [item])).toBe(true);
    });
  });

  describe('DeleteBatchInformationDto', () => {
    it('menerima payload dengan array ID numerik valid', () => {
      expect(Value.Check(DeleteBatchInformationDto, { ids: ['101', '102'] })).toBe(true);
      expect(Value.Check(DeleteBatchInformationDto, { ids: ['1'] })).toBe(true);
    });

    it('menolak payload dengan array kosong', () => {
      expect(Value.Check(DeleteBatchInformationDto, { ids: [] })).toBe(false);
    });

    it('menolak payload jika ada ID non-numerik', () => {
      expect(Value.Check(DeleteBatchInformationDto, { ids: ['101', 'abc'] })).toBe(false);
      expect(Value.Check(DeleteBatchInformationDto, { ids: [''] })).toBe(false);
    });
  });

  describe('DeleteBatchInformationResponseDto', () => {
    it('memvalidasi response deleted_count', () => {
      expect(Value.Check(DeleteBatchInformationResponseDto, { deleted_count: 5 })).toBe(true);
      expect(Value.Check(DeleteBatchInformationResponseDto, { deleted_count: 0 })).toBe(true);
      expect(Value.Check(DeleteBatchInformationResponseDto, { deleted_count: -1 })).toBe(false);
      expect(Value.Check(DeleteBatchInformationResponseDto, { deleted_count: 3.5 })).toBe(false);
    });
  });
});
