import { InformationEntity } from '@features/information/domain/entities/information.entity';
import { InformationResponseMapper } from '@features/information/presentation/mappers/information-response.mapper';
import { describe, expect, it } from 'vitest';

describe('InformationResponseMapper Unit Tests', () => {
  const dummyEntity = new InformationEntity({
    id: '101',
    title: 'Informasi Kegiatan',
    text: 'Deskripsi kegiatan kerja bakti',
    userEmail: 'guard@veranda.com',
    userName: 'Petugas Satu',
    userProfilePic: 'https://cdn.kawalo.id/pic.jpg',
    feedDt: '2026-10-01T08:00:00.000Z',
    feedFile: 'sop.pdf',
    photos: ['https://cdn.kawalo.id/p1.jpg', 'https://cdn.kawalo.id/p2.jpg'],
    feedType: 'EVENT',
    groupName: 'Cluster Sakura',
    eventLocation: 'Pos Gerbang',
    eventStartDt: '2026-10-02T08:00:00.000Z',
    eventEndDt: '2026-10-02T11:00:00.000Z',
    eventDuration: 3,
    glat: '-6.200000',
    glong: '106.816666',
  });

  describe('toResponse', () => {
    it('berhasil memetakan InformationEntity ke InformationItemDtoType', () => {
      const response = InformationResponseMapper.toResponse(dummyEntity);

      expect(response).toEqual({
        id: '101',
        title: 'Informasi Kegiatan',
        text: 'Deskripsi kegiatan kerja bakti',
        user_email: 'guard@veranda.com',
        user_name: 'Petugas Satu',
        user_profile_pic: 'https://cdn.kawalo.id/pic.jpg',
        feed_dt: '2026-10-01T08:00:00.000Z',
        feed_file: 'sop.pdf',
        photos: ['https://cdn.kawalo.id/p1.jpg', 'https://cdn.kawalo.id/p2.jpg'],
        feed_type: 'EVENT',
        group_name: 'Cluster Sakura',
        event_location: 'Pos Gerbang',
        event_start_dt: '2026-10-02T08:00:00.000Z',
        event_end_dt: '2026-10-02T11:00:00.000Z',
        event_duration: 3,
        glat: '-6.200000',
        glong: '106.816666',
      });
    });
  });

  describe('toResponseList', () => {
    it('berhasil memetakan daftar InformationEntity ke InformationItemDtoType[]', () => {
      const responses = InformationResponseMapper.toResponseList([dummyEntity]);

      expect(responses).toHaveLength(1);
      expect(responses[0]?.id).toBe('101');
      expect(responses[0]?.title).toBe('Informasi Kegiatan');
    });

    it('mengembalikan array kosong jika input list kosong', () => {
      const responses = InformationResponseMapper.toResponseList([]);
      expect(responses).toEqual([]);
    });
  });

  describe('toDeleteBatchResponse', () => {
    it('berhasil memetakan DeleteInformationOutput ke DeleteBatchInformationResponseDtoType', () => {
      const response = InformationResponseMapper.toDeleteBatchResponse({
        deletedCount: 5,
      });

      expect(response).toEqual({
        deleted_count: 5,
      });
    });
  });
});
