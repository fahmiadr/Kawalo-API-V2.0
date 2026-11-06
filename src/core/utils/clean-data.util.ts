/**
 * Anti-Corruption Layer (ACL) Sanitization Utilities.
 * Membersihkan data kotor dari database legacy MariaDB sebelum dikonversi
 * ke Domain Entities atau REST API DTOs.
 */

/**
 * Standar regex validasi format email internasional sesuai spesifikasi
 * WHATWG HTML5 Living Standard (RFC 5322 compatible subset).
 * Mencegah ReDoS dan memvalidasi format email standar dengan domain & TLD.
 */
const EMAIL_REGEX =
  /^[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)+$/;

/**
 * Helper internal untuk memeriksa apakah sebuah string merupakan format email yang valid.
 */
function isEmail(val: string): boolean {
  return EMAIL_REGEX.test(val);
}

/**
 * Membersihkan string kotor.
 * - Mengembalikan `null` jika string kosong, hanya berisi spasi, bernilai literal 'N/A', atau 'null'.
 * - Jika string terdeteksi sebagai format email yang valid, otomatis dinormalisasi menjadi lowercase.
 * - String non-email (seperti nama orang, UID grup, shift, lokasi) tetap mempertahankan casing aslinya.
 */
export function cleanString(val?: string | null): string | null {
  if (val === undefined || val === null) {
    return null;
  }
  const trimmed = val.trim();
  if (trimmed === '' || trimmed === 'N/A' || trimmed === 'null') {
    return null;
  }
  if (isEmail(trimmed)) {
    return trimmed.toLowerCase();
  }
  return trimmed;
}

/**
 * Membersihkan input string dari query/request parameter HTTP.
 * Menghapus spasi, mendeteksi string kosong, 'N/A', atau 'null'.
 * Mengembalikan `undefined` jika tidak ada input valid, sehingga siap pakai
 * untuk Prisma `where` clause dan interface opsional TypeScript tanpa perlu `?? undefined`.
 */
export function cleanQueryString(val?: string | null): string | undefined {
  const cleaned = cleanString(val);
  return cleaned ?? undefined;
}

/**
 * Membersihkan dan mengonversi nilai menjadi boolean.
 * - Menerima boolean, string ('true', 'false', '1', '0'), atau number (1, 0).
 * - Mengembalikan boolean atau null jika tidak dapat ditentukan.
 */
export function cleanBoolean(val?: unknown): boolean | null {
  if (val === undefined || val === null) {
    return null;
  }
  if (typeof val === 'boolean') {
    return val;
  }
  if (typeof val === 'number') {
    if (val === 1) return true;
    if (val === 0) return false;
    return null;
  }
  if (typeof val === 'string') {
    const trimmed = val.trim().toLowerCase();
    if (trimmed === 'true' || trimmed === '1') return true;
    if (trimmed === 'false' || trimmed === '0') return false;
    return null;
  }
  return null;
}

/**
 * Membersihkan nilai boolean dari query/request parameter HTTP.
 * Mengembalikan `undefined` jika tidak ada input valid,
 * sehingga siap pakai pada parameter DTO/Prisma tanpa perlu `?? undefined`.
 */
export function cleanQueryBoolean(val?: unknown): boolean | undefined {
  const cleaned = cleanBoolean(val);
  return cleaned ?? undefined;
}

/**
 * Membersihkan dan memvalidasi tanggal.
 * Mengabaikan tanggal epoch default database legacy (1970-01-01 atau 0000-00-00).
 * Mengembalikan string ISO 8601 yang valid atau `null`.
 */
export function cleanDate(val?: Date | string | null): string | null {
  if (val === undefined || val === null) {
    return null;
  }

  let isoStr: string;
  if (val instanceof Date) {
    if (Number.isNaN(val.getTime())) {
      return null;
    }
    isoStr = val.toISOString();
  } else {
    const trimmed = val.trim();
    if (
      trimmed === '' ||
      trimmed === 'N/A' ||
      trimmed.startsWith('0000-00-00') ||
      trimmed.startsWith('1970-01-01')
    ) {
      return null;
    }
    const parsed = new Date(trimmed);
    if (Number.isNaN(parsed.getTime())) {
      return null;
    }
    isoStr = parsed.toISOString();
  }

  // Double check hasil ISO tidak jatuh ke epoch 1970
  if (isoStr.startsWith('1970-01-01')) {
    return null;
  }

  return isoStr;
}

/**
 * Membersihkan dan memvalidasi tanggal dari query/request parameter HTTP.
 * Mengembalikan `undefined` jika tidak ada input valid atau format tidak sah,
 * sehingga aman langsung dipakai pada filter Prisma tanpa perlu `?? undefined`.
 */
export function cleanQueryDate(val?: Date | string | null): string | undefined {
  const cleaned = cleanDate(val);
  return cleaned ?? undefined;
}

/**
 * Membersihkan koordinat GPS (latitude / longitude).
 * Mengembalikan `null` jika tidak ada, bernilai 'N/A', '0', '0.0', atau kosong.
 */
export function cleanCoordinate(val?: string | null): string | null {
  const cleaned = cleanString(val);
  if (!cleaned) {
    return null;
  }
  if (cleaned === '0' || cleaned === '0.0' || cleaned === '0.000000') {
    return null;
  }
  const num = Number(cleaned);
  if (Number.isNaN(num)) {
    return null;
  }
  return cleaned;
}

/**
 * Membersihkan dan memvalidasi koordinat GPS dari query/request parameter HTTP.
 * Mengembalikan `undefined` jika tidak ada input valid, bernilai 'N/A', '0', '0.0', kosong, atau NaN,
 * sehingga aman langsung dipakai pada parameter DTO/Prisma tanpa perlu `?? undefined`.
 */
export function cleanQueryCoordinate(val?: string | null): string | undefined {
  const cleaned = cleanCoordinate(val);
  return cleaned ?? undefined;
}

/**
 * Membersihkan dan memvalidasi nilai angka (number / float).
 * - Menerima input bertipe number, string numerik, null, atau undefined.
 * - Mengabaikan NaN, Infinity, -Infinity, string kosong, atau dirty strings ('N/A', 'null').
 * - Mengembalikan `number` valid atau `null`.
 */
export function cleanNumber(val?: unknown): number | null {
  if (val === undefined || val === null) {
    return null;
  }
  if (typeof val === 'number') {
    return Number.isFinite(val) ? val : null;
  }
  if (typeof val === 'string') {
    const cleaned = cleanString(val);
    if (!cleaned) {
      return null;
    }
    const parsed = Number(cleaned);
    return Number.isFinite(parsed) ? parsed : null;
  }
  return null;
}

/**
 * Membersihkan dan memvalidasi nilai bilangan bulat (integer).
 * - Menjamin hasil berupa bilangan bulat valid tanpa pecahan desimal.
 * - Menggunakan Math.trunc untuk memotong komponen pecahan desimal.
 * - Mengembalikan integer `number` valid atau `null`.
 */
export function cleanInteger(val?: unknown): number | null {
  const num = cleanNumber(val);
  if (num === null) {
    return null;
  }
  return Math.trunc(num);
}

/**
 * Membersihkan dan memvalidasi bilangan bulat dari query/request parameter HTTP.
 * Menjamin hasil berupa integer valid tanpa pecahan desimal.
 * Mengembalikan `undefined` jika tidak ada input valid atau NaN,
 * sehingga aman langsung dipakai pada parameter DTO/Prisma tanpa perlu `?? undefined`.
 */
export function cleanQueryInteger(val?: unknown): number | undefined {
  const cleaned = cleanInteger(val);
  return cleaned ?? undefined;
}

/**
 * Mengonversi string tanggal ISO 8601 (UTC) menjadi format datetime lokal
 * `YYYY-MM-DD HH:mm:ss` yang sesuai dengan session timezone MySQL.
 *
 * **Konteks Bug:**
 * Prisma `$queryRaw` mengirimkan objek `Date` JavaScript sebagai string UTC
 * ke MySQL. Namun `DATE_FORMAT()` pada query yang sama mengelompokkan data
 * berdasarkan session timezone MySQL (misalnya UTC+8). Perbedaan ini
 * menyebabkan filter tanggal bergeser sebanyak offset timezone
 * (misalnya 8 jam mundur → record tanggal sebelumnya lolos filter).
 *
 * Fungsi ini memformat `Date` dalam timezone lokal Node.js (yang seharusnya
 * selaras dengan session timezone MySQL pada server yang sama), sehingga
 * filter `WHERE ... >= '...'` konsisten dengan pengelompokan `DATE_FORMAT()`.
 *
 * **Catatan:** Hanya untuk digunakan dalam `Prisma.sql` tagged template
 * pada `$queryRaw`. Prisma client biasa (`.findMany()`, `.where()`) sudah
 * menangani konversi timezone secara konsisten dan TIDAK memerlukan ini.
 */
export function toLocalDatetime(isoStr: string): string {
  const d = new Date(isoStr);
  if (Number.isNaN(d.getTime())) {
    return isoStr;
  }
  const pad = (n: number) => String(n).padStart(2, '0');
  return `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())} ${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`;
}

/**
 * ============================================================================
 * HTTP PUT/PATCH Mutation / Update Utilities (Tri-State Semantics)
 * ============================================================================
 * Membedakan secara tegas semantik antara:
 * 1. Omitted (`undefined`): Field tidak dikirim, jangan ubah nilai lama di DB.
 * 2. Explicit Clear (`null` / dirty / empty string): Field dikosongkan menjadi NULL di DB.
 * 3. Updated (valid value): Field diperbarui dengan nilai baru yang telah disanitasi.
 */

/**
 * Membersihkan input string dari payload mutasi HTTP (PUT/PATCH).
 * - Mengembalikan `undefined` jika `val === undefined` (field tidak disertakan dalam body).
 * - Mengembalikan `null` jika `val === null`, string kosong `""`, hanya spasi `'   '`, `'N/A'`, atau `'null'`.
 * - Mengembalikan string valid yang telah di-trim (dan dinormalisasi lowercase jika format email).
 */
export function cleanUpdateString(val?: string | null): string | null | undefined {
  if (val === undefined) {
    return undefined;
  }
  return cleanString(val);
}

/**
 * Membersihkan input koordinat GPS dari payload mutasi HTTP (PUT/PATCH).
 * - Mengembalikan `undefined` jika `val === undefined` (field tidak disertakan dalam body).
 * - Mengembalikan `null` jika `val === null`, string kosong, '0', '0.0', '0.000000', 'N/A', atau NaN.
 * - Mengembalikan string koordinat numerik valid jika ada.
 */
export function cleanUpdateCoordinate(val?: string | null): string | null | undefined {
  if (val === undefined) {
    return undefined;
  }
  return cleanCoordinate(val);
}

/**
 * Membersihkan input tanggal dari payload mutasi HTTP (PUT/PATCH).
 * - Mengembalikan `undefined` jika `val === undefined` (field tidak disertakan dalam body).
 * - Mengembalikan `null` jika `val === null`, string kosong, atau tanggal legacy epoch (1970/0000).
 * - Mengembalikan string ISO 8601 valid jika ada.
 */
export function cleanUpdateDate(val?: Date | string | null): string | null | undefined {
  if (val === undefined) {
    return undefined;
  }
  return cleanDate(val);
}

/**
 * Membersihkan input boolean dari payload mutasi HTTP (PUT/PATCH).
 * - Mengembalikan `undefined` jika `val === undefined` (field tidak disertakan dalam body).
 * - Mengembalikan `null` jika `val === null` atau tidak valid.
 * - Mengembalikan boolean valid (`true` atau `false`).
 */
export function cleanUpdateBoolean(val?: unknown): boolean | null | undefined {
  if (val === undefined) {
    return undefined;
  }
  return cleanBoolean(val);
}

/**
 * Membersihkan input bilangan desimal/number dari payload mutasi HTTP (PUT/PATCH).
 * - Mengembalikan `undefined` jika `val === undefined` (field tidak disertakan dalam body).
 * - Mengembalikan `null` jika `val === null` atau NaN/Infinity.
 * - Mengembalikan number valid.
 */
export function cleanUpdateNumber(val?: unknown): number | null | undefined {
  if (val === undefined) {
    return undefined;
  }
  return cleanNumber(val);
}

/**
 * Membersihkan input bilangan bulat/integer dari payload mutasi HTTP (PUT/PATCH).
 * - Mengembalikan `undefined` jika `val === undefined` (field tidak disertakan dalam body).
 * - Mengembalikan `null` jika `val === null` atau NaN/Infinity.
 * - Mengembalikan integer valid tanpa desimal.
 */
export function cleanUpdateInteger(val?: unknown): number | null | undefined {
  if (val === undefined) {
    return undefined;
  }
  return cleanInteger(val);
}

