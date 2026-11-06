import { randomBytes, randomUUID } from 'node:crypto';

/**
 * Menghasilkan UUIDv4 murni sesuai spesifikasi RFC 4122.
 * Format: 8-4-4-4-12 hex string (versi 4, varian RFC 4122).
 */
export function generateUuidV4(): string {
  return randomUUID();
}

/**
 * Menghasilkan UUIDv7 murni sesuai spesifikasi RFC 9562 (Section 5.7).
 * - 48 bit timestamp Unix epoch (ms)
 * - 4 bit versi (0b0111 = 7)
 * - 12 bit pseudo-random sequence
 * - 2 bit varian (0b10)
 * - 62 bit pseudo-random sequence
 * Total: 128 bit dalam format canonical hex 36 karakter (8-4-4-4-12).
 */
export function generateUuidV7(): string {
  const bytes = randomBytes(16);
  const now = BigInt(Date.now());

  // 48-bit timestamp in milliseconds (Big-Endian)
  bytes[0] = Number((now >> 40n) & 0xffn);
  bytes[1] = Number((now >> 32n) & 0xffn);
  bytes[2] = Number((now >> 24n) & 0xffn);
  bytes[3] = Number((now >> 16n) & 0xffn);
  bytes[4] = Number((now >> 8n) & 0xffn);
  bytes[5] = Number(now & 0xffn);

  // Version 7: 0111xxxx
  bytes[6] = ((bytes[6] ?? 0) & 0x0f) | 0x70;

  // Variant RFC 4122/9562: 10xxxxxx
  bytes[8] = ((bytes[8] ?? 0) & 0x3f) | 0x80;

  const hex = bytes.toString('hex');
  return `${hex.slice(0, 8)}-${hex.slice(8, 12)}-${hex.slice(12, 16)}-${hex.slice(16, 20)}-${hex.slice(20)}`;
}

/**
 * Menghasilkan Prefixed UUIDv7 sesuai standar TypeID / Stripe tanpa merusak bit-level UUIDv7.
 * - Mempertahankan sifat K-sortable (time-ordered) pada B-Tree index karena prefix bernilai konstan.
 * - Format: `${prefix}_${uuidv7}`
 *
 * Contoh: generatePrefixedUuidV7('feed') -> 'feed_01925b4a-4e2b-7abc-8def-0123456789ab'
 */
export function generatePrefixedUuidV7(prefix: string): string {
  return `${prefix}_${generateUuidV7()}`;
}
