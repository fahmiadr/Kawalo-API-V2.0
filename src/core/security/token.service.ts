import { EnvConfig } from '@core/config/env.config';
import { AppConstants } from '@core/constants/app.constants';
import { RsaKeyManager } from '@core/security/rsa-key.manager';
import { cleanInteger, cleanQueryString } from '@core/utils/clean-data.util';
import crypto from 'crypto';
import * as jose from 'jose';
import type { JwtUserPayload } from '@core/types/jwt-payload.types';

export class TokenService {
  /**
   * Issue a short-lived RS256 Access Token (Signed JWT)
   */
  public static async signAccessToken(payload: JwtUserPayload): Promise<{
    token: string;
    expiresIn: number;
  }> {
    const privateKey = RsaKeyManager.getPrivateKey();
    const expiresIn = EnvConfig.JWT_EXPIRES_IN; // 900s = 15m

    if (typeof payload.session_ver !== 'number' || payload.session_ver <= 0) {
      throw new Error(
        'session_ver wajib berupa bilangan bulat positif untuk menerbitkan Access Token',
      );
    }

    const jwtBuilder = new jose.SignJWT({
      email: payload.email,
      name: payload.name,
      role: payload.role,
      group_uid: payload.group_uid,
      allow_admin: payload.allow_admin,
      allow_patrol: payload.allow_patrol,
      allow_mon_patrol: payload.allow_mon_patrol,
      session_ver: payload.session_ver,
    })
      .setProtectedHeader({
        alg: 'RS256',
        typ: 'JWT',
        kid: RsaKeyManager.getActiveKeyId(),
      })
      .setIssuer(AppConstants.JWT_ISSUER)
      .setAudience(AppConstants.JWT_AUDIENCE)
      .setIssuedAt()
      .setExpirationTime(`${expiresIn}s`);

    if (payload.sub) {
      jwtBuilder.setSubject(payload.sub);
    }

    const jwt = await jwtBuilder.sign(privateKey);

    return {
      token: jwt,
      expiresIn,
    };
  }

  /**
   * Verify an RS256 Access Token
   */
  public static async verifyAccessToken(token: string): Promise<JwtUserPayload | null> {
    try {
      const header = jose.decodeProtectedHeader(token);
      const kid = header?.kid;
      const publicKey = RsaKeyManager.getPublicKey(kid);
      const { payload } = await jose.jwtVerify(token, publicKey, {
        issuer: AppConstants.JWT_ISSUER,
        audience: AppConstants.JWT_AUDIENCE,
        algorithms: ['RS256'],
      });

      // Strict Validation (Zero Backward-Compatibility):
      // Token wajib memiliki klaim session_ver yang valid berupa bilangan bulat positif.
      const sessionVer = cleanInteger(payload.session_ver as number);
      if (sessionVer === null || sessionVer <= 0) {
        return null;
      }

      return {
        sub: cleanQueryString(payload.sub as string),
        email: cleanQueryString(payload.email as string),
        name: cleanQueryString(payload.name as string),
        role: cleanQueryString(payload.role as string)?.toUpperCase(),
        group_uid: cleanQueryString(payload.group_uid as string),
        allow_admin: Boolean(payload.allow_admin),
        allow_patrol: Boolean(payload.allow_patrol),
        allow_mon_patrol: Boolean(payload.allow_mon_patrol),
        session_ver: sessionVer,
      };
    } catch {
      return null;
    }
  }

  /**
   * Generate an opaque 64-character hexadecimal CSPRNG refresh token string
   */
  public static generateRefreshToken(): string {
    return crypto.randomBytes(32).toString('hex');
  }

  /**
   * Compute a one-way SHA-256 hex hash of the raw refresh token for safe storage
   */
  public static hashToken(rawToken: string): string {
    return crypto.createHash('sha256').update(rawToken).digest('hex');
  }
}
