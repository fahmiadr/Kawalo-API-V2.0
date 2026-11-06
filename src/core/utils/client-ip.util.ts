import { EnvConfig } from '@core/config/env.config';

/**
 * Mengubah string IPv4 menjadi integer 32-bit unsigned.
 */
function ipv4ToInt(ip: string): number {
  return (
    ip.split('.').reduce((acc, octet) => ((acc << 8) + Number.parseInt(octet, 10)) >>> 0, 0) >>> 0
  );
}

/**
 * Membersihkan format IP dari format IPv4-mapped IPv6 (e.g., "::ffff:192.168.1.1").
 */
export function normalizeIp(rawIp: string): string {
  const trimmed = rawIp.trim();
  if (trimmed.startsWith('::ffff:')) {
    return trimmed.substring(7);
  }
  return trimmed;
}

/**
 * Memeriksa apakah suatu IP berada di dalam subnet CIDR (khusus IPv4 atau loopback IPv6).
 */
export function isIpInCidr(ip: string, cidr: string): boolean {
  const cleanIp = normalizeIp(ip);
  const cleanCidr = cidr.trim();

  // Handle IPv6 loopback
  if (cleanCidr === '::1' || cleanCidr === '::1/128') {
    return cleanIp === '::1' || cleanIp === '0:0:0:0:0:0:0:1';
  }

  // Jika CIDR IPv4
  const parts = cleanCidr.split('/');
  const range = parts[0] || '';
  const bits = parts[1] || '32';
  const prefix = Number.parseInt(bits, 10);

  // Validasi format IPv4 sederhana
  const ipv4Regex = /^(\d{1,3}\.){3}\d{1,3}$/;
  if (!ipv4Regex.test(cleanIp) || !ipv4Regex.test(range)) {
    return cleanIp === range;
  }

  if (prefix === 0) return true;
  if (prefix < 0 || prefix > 32) return false;

  const mask = prefix === 32 ? 0xffffffff : ~((1 << (32 - prefix)) - 1) >>> 0;
  return (ipv4ToInt(cleanIp) & mask) === (ipv4ToInt(range) & mask);
}

/**
 * Memeriksa apakah IP berasal dari daftar subnet reverse proxy yang terpercaya.
 */
export function isTrustedProxy(ip: string, trustedCidrs = EnvConfig.TRUSTED_PROXY_CIDRS): boolean {
  const cleanIp = normalizeIp(ip);
  if (!cleanIp) return false;

  return trustedCidrs.some((cidr) => isIpInCidr(cleanIp, cidr));
}

/**
 * Menyelesaikan Client IP asli secara aman (*Safe Client IP Resolution*).
 *
 * Mencegah pemalsuan header (IP Spoofing) dengan algoritma:
 * 1. Jika socket connection IP tidak berasal dari trusted proxy, abaikan semua header X-Forwarded-* / X-Real-IP.
 * 2. Jika berasal dari trusted proxy, telusuri `X-Forwarded-For` dari kanan ke kiri (RFC 7239),
 *    abaikan hop proxy internal, dan ambil IP eksternal pertama yang bukan trusted proxy.
 * 3. Fallback ke `X-Real-IP` yang dipasang Nginx/Traefik jika X-Forwarded-For tidak tersedia.
 */
export function resolveClientIp(
  request: Request,
  socketRemoteAddress?: string,
  trustedCidrs = EnvConfig.TRUSTED_PROXY_CIDRS,
): string {
  const rawSocket =
    socketRemoteAddress ||
    (request as unknown as { ip?: string }).ip ||
    (request as unknown as { socket?: { remoteAddress?: string } }).socket?.remoteAddress;

  const socketIp = rawSocket ? normalizeIp(rawSocket) : '127.0.0.1';

  // Cek apakah socket remote address terdaftar sebagai proxy terpercaya
  const socketIsTrusted = isTrustedProxy(socketIp, trustedCidrs);

  // Jika request tidak melalui trusted proxy (misal akses langsung ke port Node/Bun),
  // header X-Forwarded-For dan X-Real-IP dilarang dipercaya sama sekali!
  if (!socketIsTrusted && rawSocket) {
    return socketIp;
  }

  // Jika melalui trusted proxy, periksa X-Forwarded-For terlebih dahulu
  const xForwardedFor = request.headers.get('x-forwarded-for');
  if (xForwardedFor) {
    const rawIps = xForwardedFor
      .split(',')
      .map((ip) => normalizeIp(ip))
      .filter(Boolean);

    // Telusuri dari kanan ke kiri (hop terdekat ke server)
    for (let i = rawIps.length - 1; i >= 0; i--) {
      const candidateIp = rawIps[i];
      if (candidateIp && !isTrustedProxy(candidateIp, trustedCidrs)) {
        return candidateIp;
      }
    }

    // Jika seluruh hop adalah trusted proxy, ambil yang terluar (paling kiri)
    if (rawIps.length > 0 && rawIps[0]) {
      return rawIps[0];
    }
  }

  // Periksa X-Real-IP jika disematkan oleh Nginx
  const xRealIp = request.headers.get('x-real-ip');
  if (xRealIp) {
    return normalizeIp(xRealIp);
  }

  return socketIp;
}
