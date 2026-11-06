export interface JwtUserPayload {
  sub?: string;
  email?: string;
  name?: string;
  role?: string | null;
  group_uid?: string | null;
  allow_admin?: boolean;
  allow_patrol?: boolean;
  allow_mon_patrol?: boolean;
  session_ver: number;
}

/**
 * Representasi klaim personil terautentikasi yang valid.
 * Menjamin `sub` dan `email` tidak bernilai undefined sehingga handler terproteksi
 * tidak memerlukan non-null assertions (!).
 */
export interface AuthenticatedUserPayload extends JwtUserPayload {
  sub: string;
  email: string;
  session_ver: number;
}
