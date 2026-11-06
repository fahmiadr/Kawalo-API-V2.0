import {
  FloorDeniedFrameSchema,
  FloorGrantedFrameSchema,
  FloorReleasedFrameSchema,
  FloorRequestFrameSchema,
  FloorResetFrameSchema,
  FloorRevokedFrameSchema,
  FloorStateChangedFrameSchema,
  InboundPttControlFrameSchema,
  MemberCountUpdatedFrameSchema,
} from '@features/ptt/dto/ptt-frame.dto';
import { Value } from '@sinclair/typebox/value';
import { describe, expect, it } from 'vitest';

describe('PTT Wire Frame DTO Schemas Validation Tests', () => {
  describe('FloorRequestFrameSchema (Security BLK-01 Verification)', () => {
    it('harus menerima FLOOR_REQUEST yang valid dengan requestId tanpa atribut role', () => {
      const valid = {
        type: 'FLOOR_REQUEST',
        requestId: 'req_01HX98Z7PQ',
      };
      expect(Value.Check(FloorRequestFrameSchema, valid)).toBe(true);
    });

    it('harus menolak FLOOR_REQUEST dengan requestId kosong', () => {
      const invalid = {
        type: 'FLOOR_REQUEST',
        requestId: '',
      };
      expect(Value.Check(FloorRequestFrameSchema, invalid)).toBe(false);
    });
  });

  describe('FloorGrantedFrameSchema', () => {
    it('harus menerima FLOOR_GRANTED dengan leaseTtlMs', () => {
      const valid = {
        type: 'FLOOR_GRANTED',
        requestId: 'req_01HX98Z7PQ',
        leaseTtlMs: 2000,
      };
      expect(Value.Check(FloorGrantedFrameSchema, valid)).toBe(true);
    });
  });

  describe('FloorDeniedFrameSchema', () => {
    it('harus menerima FLOOR_DENIED dengan alasan CHANNEL_BUSY dan metadata currentSpeaker', () => {
      const valid = {
        type: 'FLOOR_DENIED',
        requestId: 'req_01HX98Z7PQ',
        reason: 'CHANNEL_BUSY',
        currentSpeaker: {
          userId: 'usr_sec_102',
          name: 'Budi Santoso',
          role: 'GUARD',
        },
      };
      expect(Value.Check(FloorDeniedFrameSchema, valid)).toBe(true);
    });

    it('harus menerima FLOOR_DENIED dengan alasan INSUFFICIENT_PRIORITY', () => {
      const valid = {
        type: 'FLOOR_DENIED',
        requestId: 'req_01HX98Z7PQ',
        reason: 'INSUFFICIENT_PRIORITY',
      };
      expect(Value.Check(FloorDeniedFrameSchema, valid)).toBe(true);
    });
  });

  describe('FloorStateChangedFrameSchema (Nullability & Presence Verification)', () => {
    it('harus menerima FLOOR_STATE_CHANGED saat TRANSMITTING dengan objek speaker dan memberCount', () => {
      const valid = {
        type: 'FLOOR_STATE_CHANGED',
        state: 'TRANSMITTING',
        speaker: {
          userId: 'usr_sec_102',
          name: 'Budi Santoso',
          role: 'GUARD',
        },
        memberCount: 14,
      };
      expect(Value.Check(FloorStateChangedFrameSchema, valid)).toBe(true);
    });

    it('harus menerima FLOOR_STATE_CHANGED saat IDLE dengan speaker: null eksplisit', () => {
      const valid = {
        type: 'FLOOR_STATE_CHANGED',
        state: 'IDLE',
        speaker: null,
        memberCount: 14,
      };
      expect(Value.Check(FloorStateChangedFrameSchema, valid)).toBe(true);
    });

    it('harus menolak FLOOR_STATE_CHANGED tanpa memberCount', () => {
      const invalid = {
        type: 'FLOOR_STATE_CHANGED',
        state: 'IDLE',
        speaker: null,
      };
      expect(Value.Check(FloorStateChangedFrameSchema, invalid)).toBe(false);
    });
  });

  describe('FloorReleasedFrameSchema & FloorRevokedFrameSchema', () => {
    it('harus menerima FLOOR_RELEASED', () => {
      const valid = {
        type: 'FLOOR_RELEASED',
        reason: 'BUTTON_RELEASED',
      };
      expect(Value.Check(FloorReleasedFrameSchema, valid)).toBe(true);
    });

    it('harus menerima FLOOR_REVOKED dengan reason ADMIN_OVERRIDE', () => {
      const valid = {
        type: 'FLOOR_REVOKED',
        reason: 'ADMIN_OVERRIDE',
        preemptedBy: 'Captain Hendra',
      };
      expect(Value.Check(FloorRevokedFrameSchema, valid)).toBe(true);
    });

    it('harus menerima FLOOR_REVOKED dengan reason TIME_OUT_TIMER_EXPIRED', () => {
      const valid = {
        type: 'FLOOR_REVOKED',
        reason: 'TIME_OUT_TIMER_EXPIRED',
      };
      expect(Value.Check(FloorRevokedFrameSchema, valid)).toBe(true);
    });
  });

  describe('FloorResetFrameSchema & MemberCountUpdatedFrameSchema', () => {
    it('harus menerima FLOOR_RESET dengan reason PREEMPTION_FLUSH', () => {
      const valid = {
        type: 'FLOOR_RESET',
        reason: 'PREEMPTION_FLUSH',
      };
      expect(Value.Check(FloorResetFrameSchema, valid)).toBe(true);
    });

    it('harus menerima MEMBER_COUNT_UPDATED', () => {
      const valid = {
        type: 'MEMBER_COUNT_UPDATED',
        memberCount: 15,
      };
      expect(Value.Check(MemberCountUpdatedFrameSchema, valid)).toBe(true);
    });
  });

  describe('InboundPttControlFrameSchema Union', () => {
    it('harus memvalidasi union frame inbound control yang masuk dari klien', () => {
      expect(
        Value.Check(InboundPttControlFrameSchema, {
          type: 'FLOOR_REQUEST',
          requestId: 'req_123',
        }),
      ).toBe(true);

      expect(
        Value.Check(InboundPttControlFrameSchema, {
          type: 'FLOOR_RELEASED',
        }),
      ).toBe(true);

      expect(
        Value.Check(InboundPttControlFrameSchema, {
          type: 'UNKNOWN_TYPE',
        }),
      ).toBe(false);
    });
  });
});
