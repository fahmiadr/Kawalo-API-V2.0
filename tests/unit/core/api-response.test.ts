import { EnvConfig } from '@core/config/env.config';
import { ErrorCodes } from '@core/constants/error.codes';
import { Failure } from '@core/error/failure';
import { ApiResponse } from '@core/utils/api-response';
import { describe, expect, it } from 'vitest';

describe('ApiResponse Utility Unit Tests', () => {
  describe('success', () => {
    it('harus membuat response sukses dengan format standar', () => {
      const data = { id: 123, title: 'Item 1' };
      const response = ApiResponse.success(data, 'Data berhasil dimuat');

      expect(response.success).toBe(true);
      expect(response.message).toBe('Data berhasil dimuat');
      expect(response.data).toEqual(data);
    });

    it('harus menggunakan default message "Berhasil" jika tidak diberikan', () => {
      const response = ApiResponse.success(['a', 'b']);
      expect(response.success).toBe(true);
      expect(response.message).toBe('Berhasil');
    });
  });

  describe('error', () => {
    it('harus membuat response error dengan error_code yang sesuai', () => {
      const response = ApiResponse.error(
        'Format tidak sah',
        ErrorCodes.VALIDATION_ERROR,
        'Field title is required',
      );

      expect(response.success).toBe(false);
      expect(response.message).toBe('Format tidak sah');
      expect(response.error_code).toBe(ErrorCodes.VALIDATION_ERROR);

      if (EnvConfig.isProduction) {
        expect(response.details).toBeUndefined();
      } else {
        expect(response.details).toBe('Field title is required');
      }
    });
  });

  describe('fromFailure', () => {
    it('harus memetakan instance Failure ke ErrorApiResponse secara akurat', () => {
      const failure = Failure.notFound('Data user tidak ditemukan');
      const response = ApiResponse.fromFailure(failure);

      expect(response.success).toBe(false);
      expect(response.message).toBe('Data user tidak ditemukan');
      expect(response.error_code).toBe(ErrorCodes.RESOURCE_NOT_FOUND);
    });

    it('harus meneruskan details dari Failure pada environment non-production', () => {
      const failure = Failure.validation('Input tidak valid', { field: 'email' });
      const response = ApiResponse.fromFailure(failure);

      if (EnvConfig.isProduction) {
        expect(response.details).toBeUndefined();
      } else {
        expect(response.details).toEqual({ field: 'email' });
      }
    });
  });

  describe('static mappings', () => {
    it('harus mengekspos ErrorCodes dan InvertedHttpStatusMap', () => {
      expect(ApiResponse.errorCode).toBe(ErrorCodes);
      expect(ApiResponse.status[200]).toBe('OK');
      expect(ApiResponse.status[401]).toBe('Unauthorized');
      expect(ApiResponse.status[404]).toBe('Not Found');
      expect(ApiResponse.status[500]).toBe('Internal Server Error');
    });
  });
});
