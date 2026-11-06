import {
  isIpInCidr,
  isTrustedProxy,
  normalizeIp,
  resolveClientIp,
} from '@core/utils/client-ip.util';
import { describe, expect, it } from 'vitest';

describe('Safe Client IP Resolver (client-ip.util)', () => {
  describe('normalizeIp', () => {
    it('harus menormalisasi IPv4-mapped IPv6 (::ffff:x.x.x.x)', () => {
      expect(normalizeIp('::ffff:192.168.1.50')).toBe('192.168.1.50');
      expect(normalizeIp('::ffff:10.0.0.1')).toBe('10.0.0.1');
    });

    it('harus mempertahankan IPv4 dan IPv6 standar', () => {
      expect(normalizeIp('192.168.1.1')).toBe('192.168.1.1');
      expect(normalizeIp('::1')).toBe('::1');
    });
  });

  describe('isIpInCidr', () => {
    it('harus mencocokkan subnet loopback IPv4 dan IPv6', () => {
      expect(isIpInCidr('127.0.0.1', '127.0.0.1/32')).toBe(true);
      expect(isIpInCidr('127.0.0.2', '127.0.0.0/8')).toBe(true);
      expect(isIpInCidr('::1', '::1/128')).toBe(true);
      expect(isIpInCidr('192.168.1.1', '127.0.0.1/32')).toBe(false);
    });

    it('harus memvalidasi subnet private class A, B, C', () => {
      expect(isIpInCidr('10.254.1.2', '10.0.0.0/8')).toBe(true);
      expect(isIpInCidr('11.0.0.1', '10.0.0.0/8')).toBe(false);

      expect(isIpInCidr('172.18.0.5', '172.16.0.0/12')).toBe(true);
      expect(isIpInCidr('172.33.0.1', '172.16.0.0/12')).toBe(false);

      expect(isIpInCidr('192.168.1.100', '192.168.0.0/16')).toBe(true);
      expect(isIpInCidr('192.169.1.1', '192.168.0.0/16')).toBe(false);
    });
  });

  describe('isTrustedProxy', () => {
    it('harus mengembalikan true jika IP termasuk subnet proxy terpercaya', () => {
      expect(isTrustedProxy('127.0.0.1')).toBe(true);
      expect(isTrustedProxy('10.0.0.1')).toBe(true);
      expect(isTrustedProxy('172.20.0.2')).toBe(true);
      expect(isTrustedProxy('203.0.113.195')).toBe(false);
    });
  });

  describe('resolveClientIp', () => {
    it('harus mengembalikan socket remote address jika request tidak melalui trusted proxy (mencegah spoofing)', () => {
      const untrustedSocket = '203.0.113.5';
      const req = new Request('http://localhost/test', {
        headers: {
          'x-forwarded-for': '8.8.8.8, 1.1.1.1',
          'x-real-ip': '8.8.8.8',
        },
      });

      // Karena socket IP bukan proxy terpercaya, header palsu diabaikan
      expect(resolveClientIp(req, untrustedSocket)).toBe('203.0.113.5');
    });

    it('harus mengambil real client IP dari X-Forwarded-For saat melalui Nginx terpercaya', () => {
      const trustedNginxSocket = '127.0.0.1';
      // Format: [client_asli, proxy_internal]
      const req = new Request('http://localhost/test', {
        headers: {
          'x-forwarded-for': '182.253.120.44, 10.0.0.5',
        },
      });

      // Hop 10.0.0.5 adalah proxy terpercaya, maka yang diambil adalah 182.253.120.44
      expect(resolveClientIp(req, trustedNginxSocket)).toBe('182.253.120.44');
    });

    it('harus mengabaikan IP palsu yang diprepend oleh penyerang', () => {
      const trustedNginxSocket = '172.18.0.2';
      // Penyerang mencoba menyuntikkan 1.1.1.1 di awal, tapi Nginx menambahkan client asli 203.0.113.50 di akhir
      const req = new Request('http://localhost/test', {
        headers: {
          'x-forwarded-for': '1.1.1.1, 203.0.113.50, 10.0.0.2',
        },
      });

      // Menelusuri dari kanan: 10.0.0.2 (trusted proxy) -> 203.0.113.50 (untrusted = client asli!)
      expect(resolveClientIp(req, trustedNginxSocket)).toBe('203.0.113.50');
    });

    it('harus fallback ke X-Real-IP jika disematkan oleh Nginx', () => {
      const trustedNginxSocket = '127.0.0.1';
      const req = new Request('http://localhost/test', {
        headers: {
          'x-real-ip': '114.122.50.88',
        },
      });

      expect(resolveClientIp(req, trustedNginxSocket)).toBe('114.122.50.88');
    });

    it('harus membaca socket IP dari properti request.ip (Elysia node adapter) dan menolak spoofing', () => {
      const req = new Request('http://localhost/test', {
        headers: {
          'x-forwarded-for': '127.0.0.1, 192.168.1.1',
          'x-real-ip': '127.0.0.1',
        },
      });
      (req as unknown as { ip: string }).ip = '::ffff:203.0.113.88';

      expect(resolveClientIp(req)).toBe('203.0.113.88');
    });
  });
});
