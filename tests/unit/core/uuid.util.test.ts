import { describe, expect, it } from 'vitest';
import { generatePrefixedUuidV7, generateUuidV4, generateUuidV7 } from '@core/utils/uuid.util';

describe('UUIDv7 Utility Unit Tests', () => {
  it('generateUuidV7 menghasilkan UUIDv7 valid sesuai spesifikasi RFC 9562', () => {
    const uuid = generateUuidV7();

    // Format 8-4-4-4-12 hex string
    expect(uuid).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);

    // Versi 7 pada karakter ke-15
    expect(uuid[14]).toBe('7');

    // Varian RFC 9562 (8, 9, a, atau b) pada karakter ke-20
    expect(['8', '9', 'a', 'b']).toContain(uuid[19]?.toLowerCase());
  });

  it('timestamp yang disematkan pada UUIDv7 mendekati waktu sekarang', () => {
    const before = Date.now();
    const uuid = generateUuidV7();
    const after = Date.now();

    // Ekstrak 48-bit timestamp dari 12 karakter hex pertama (8 + 4 tanpa hyphen)
    const timeHex = uuid.slice(0, 8) + uuid.slice(9, 13);
    const extractedTime = Number(BigInt(`0x${timeHex}`));

    expect(extractedTime).toBeGreaterThanOrEqual(before);
    expect(extractedTime).toBeLessThanOrEqual(after);
  });

  it('generatePrefixedUuidV7 menghasilkan string berformat ${prefix}_${uuidv7}', () => {
    const prefixed = generatePrefixedUuidV7('feed');

    expect(prefixed.startsWith('feed_')).toBe(true);

    const rawUuid = prefixed.slice(5);
    expect(rawUuid).toMatch(
      /^[0-9a-f]{8}-[0-9a-f]{4}-7[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i,
    );
    expect(prefixed.length).toBe(41); // 5 (feed_) + 36 (UUID) = 41 chars <= 64 chars
  });

  it('UUIDv7 berurutan secara leksikografis (K-sortable)', async () => {
    const id1 = generatePrefixedUuidV7('feed');
    // Berikan jeda kecil untuk memastikan increment waktu
    await new Promise((resolve) => setTimeout(resolve, 5));
    const id2 = generatePrefixedUuidV7('feed');

    expect(id2 > id1).toBe(true);
  });

  it('generateUuidV4 menghasilkan UUIDv4 valid sesuai spesifikasi RFC 4122', () => {
    const uuid = generateUuidV4();

    expect(uuid).toMatch(/^[0-9a-f]{8}-[0-9a-f]{4}-4[0-9a-f]{3}-[89ab][0-9a-f]{3}-[0-9a-f]{12}$/i);
    expect(uuid[14]).toBe('4');
    expect(['8', '9', 'a', 'b']).toContain(uuid[19]?.toLowerCase());
  });
});
