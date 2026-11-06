import crypto from 'crypto';
import fs from 'fs';
import * as jose from 'jose';
import path from 'path';
import { logger } from '@core/utils/logger.service';

export interface KeyStoreEntry {
  kid: string;
  privateKeyObj?: crypto.KeyObject;
  publicKeyObj: crypto.KeyObject;
  jwk: jose.JWK;
  archivedAtMs?: number;
}

export class RsaKeyManager {
  private static activeKeyId = '';
  private static keyStore: Map<string, KeyStoreEntry> = new Map();
  private static jwksCache: { keys: jose.JWK[] } | null = null;
  private static initPromise: Promise<void> | null = null;

  /**
   * Batas maksimal umur kunci arsip yang ditampilkan pada endpoint JWKS publik (/.well-known/jwks.json).
   * Default: 7 hari. File di storage fisik (certs/archived/) tetap disimpan selama 30 hari untuk audit trail.
   */
  private static jwksMaxAgeDays = 7;

  public static getJwksMaxAgeDays(): number {
    return RsaKeyManager.jwksMaxAgeDays;
  }

  public static setJwksMaxAgeDays(days: number): void {
    RsaKeyManager.jwksMaxAgeDays = days;
    RsaKeyManager.rebuildJwksCache();
  }

  /**
   * Menghasilkan Key ID (KID) default dengan format seragam berbasis epoch timestamp:
   * kawalo-key-<timestamp> (contoh: kawalo-key-1789882681000)
   */
  public static generateKeyId(): string {
    return `kawalo-key-${Date.now()}`;
  }

  public static async init(): Promise<void> {
    if (RsaKeyManager.initPromise) {
      return RsaKeyManager.initPromise;
    }
    RsaKeyManager.initPromise = RsaKeyManager.doInit();
    try {
      await RsaKeyManager.initPromise;
    } finally {
      RsaKeyManager.initPromise = null;
    }
  }

  private static async doInit(): Promise<void> {
    const certsDir = path.resolve(process.cwd(), 'certs');
    const archiveDir = path.join(certsDir, 'archived');
    const privateKeyPath = path.join(certsDir, 'private.key');
    const publicKeyPath = path.join(certsDir, 'public.key');
    const activeKidPath = path.join(certsDir, 'active_kid.txt');

    if (!fs.existsSync(certsDir)) {
      fs.mkdirSync(certsDir, { recursive: true });
    }
    if (!fs.existsSync(archiveDir)) {
      fs.mkdirSync(archiveDir, { recursive: true });
    }

    // Determine active key ID from file if present
    let hasExistingKid = false;
    if (fs.existsSync(activeKidPath)) {
      const savedKid = fs.readFileSync(activeKidPath, 'utf8').trim();
      if (savedKid) {
        RsaKeyManager.activeKeyId = savedKid;
        hasExistingKid = true;
      }
    }

    let regenerate = !fs.existsSync(privateKeyPath) || !fs.existsSync(publicKeyPath);

    // If existing key is not 4096-bit, regenerate it to satisfy 4096-bit requirement
    if (!regenerate) {
      try {
        const existingPem = fs.readFileSync(publicKeyPath, 'utf8');
        const keyObj = crypto.createPublicKey(existingPem);
        if (keyObj.asymmetricKeyDetails?.modulusLength !== 4096) {
          logger.info(
            '[RsaKeyManager] Upgrading RSA keypair to 4096-bit per security requirement...',
          );
          regenerate = true;
        }
      } catch {
        regenerate = true;
      }
    }

    if (regenerate) {
      // Jika belum ada KID yang tersimpan di disk, gunakan format konsisten kawalo-key-<timestamp>
      if (!hasExistingKid) {
        RsaKeyManager.activeKeyId = RsaKeyManager.generateKeyId();
      }

      logger.info(
        `[RsaKeyManager] Generating new RSA 4096-bit keypair (${RsaKeyManager.activeKeyId}) for RS256 signing...`,
      );
      const { privateKey, publicKey } = crypto.generateKeyPairSync('rsa', {
        modulusLength: 4096,
        publicKeyEncoding: {
          type: 'spki',
          format: 'pem',
        },
        privateKeyEncoding: {
          type: 'pkcs8',
          format: 'pem',
        },
      });

      fs.writeFileSync(privateKeyPath, privateKey, 'utf8');
      fs.writeFileSync(publicKeyPath, publicKey, 'utf8');
      fs.writeFileSync(activeKidPath, RsaKeyManager.activeKeyId, 'utf8');
    }

    const activePrivatePem = fs.readFileSync(privateKeyPath, 'utf8');
    const activePublicPem = fs.readFileSync(publicKeyPath, 'utf8');

    const activePrivateObj = crypto.createPrivateKey(activePrivatePem);
    const activePublicObj = crypto.createPublicKey(activePublicPem);

    const activeJwk = await jose.exportJWK(activePublicObj);
    activeJwk.kid = RsaKeyManager.activeKeyId;
    activeJwk.use = 'sig';
    activeJwk.alg = 'RS256';

    RsaKeyManager.keyStore.clear();
    RsaKeyManager.keyStore.set(RsaKeyManager.activeKeyId, {
      kid: RsaKeyManager.activeKeyId,
      privateKeyObj: activePrivateObj,
      publicKeyObj: activePublicObj,
      jwk: activeJwk,
    });

    // Load any archived public keys for graceful verification during key rotation
    const archiveFiles = fs.readdirSync(archiveDir);
    for (const file of archiveFiles) {
      if (file.endsWith('.pub.pem') || file.endsWith('.public.key')) {
        try {
          const kid = file.replace(/\.(pub\.pem|public\.key)$/, '');
          const filePath = path.join(archiveDir, file);
          const archivedPublicPem = fs.readFileSync(filePath, 'utf8');
          const archivedPublicObj = crypto.createPublicKey(archivedPublicPem);
          const archivedJwk = await jose.exportJWK(archivedPublicObj);
          archivedJwk.kid = kid;
          archivedJwk.use = 'sig';
          archivedJwk.alg = 'RS256';

          const stats = fs.statSync(filePath);
          let archivedAtMs = stats.mtimeMs;
          const tsMatch = kid.match(/^kawalo-key-(\d+)$/);
          if (tsMatch?.[1]) {
            const parsedTs = Number.parseInt(tsMatch[1], 10);
            if (!Number.isNaN(parsedTs) && parsedTs > 0) {
              archivedAtMs = parsedTs;
            }
          }

          RsaKeyManager.keyStore.set(kid, {
            kid,
            publicKeyObj: archivedPublicObj,
            jwk: archivedJwk,
            archivedAtMs,
          });
        } catch (err) {
          logger.warn(
            { err },
            `[RsaKeyManager] Failed to load archived key ${file}: ${err instanceof Error ? err.message : String(err)}`,
          );
        }
      }
    }

    RsaKeyManager.rebuildJwksCache();
  }

  public static getActiveKeyId(): string {
    return RsaKeyManager.activeKeyId;
  }

  public static getPrivateKey(kid?: string): crypto.KeyObject {
    if (kid) {
      const entry = RsaKeyManager.keyStore.get(kid);
      if (entry?.privateKeyObj) return entry.privateKeyObj;
    }
    const activeEntry = RsaKeyManager.keyStore.get(RsaKeyManager.activeKeyId);
    if (activeEntry?.privateKeyObj) return activeEntry.privateKeyObj;

    for (const entry of RsaKeyManager.keyStore.values()) {
      if (entry.privateKeyObj) return entry.privateKeyObj;
    }

    // Synchronous fallback if called before init finishes or during test transitions
    try {
      const certsDir = path.resolve(process.cwd(), 'certs');
      const privateKeyPath = path.join(certsDir, 'private.key');
      if (fs.existsSync(privateKeyPath)) {
        const pem = fs.readFileSync(privateKeyPath, 'utf8');
        return crypto.createPrivateKey(pem);
      }
    } catch (_) {}

    throw new Error(
      `RsaKeyManager: Private key for kid "${kid || RsaKeyManager.activeKeyId}" not found.`,
    );
  }

  private static rotationPromise: Promise<{ activeKid: string; retiredKid: string }> | null = null;

  public static getPublicKey(kid?: string): crypto.KeyObject {
    if (kid) {
      const entry = RsaKeyManager.keyStore.get(kid);
      if (entry?.publicKeyObj) return entry.publicKeyObj;
    }
    const activeEntry = RsaKeyManager.keyStore.get(RsaKeyManager.activeKeyId);
    if (activeEntry?.publicKeyObj) return activeEntry.publicKeyObj;

    for (const entry of RsaKeyManager.keyStore.values()) {
      if (entry.publicKeyObj) return entry.publicKeyObj;
    }

    try {
      const certsDir = path.resolve(process.cwd(), 'certs');
      // If a specific kid was requested, check archived directory as well
      if (kid) {
        const archivedPath = path.join(certsDir, 'archived', `${kid}.pub.pem`);
        if (fs.existsSync(archivedPath)) {
          const pem = fs.readFileSync(archivedPath, 'utf8');
          const keyObj = crypto.createPublicKey(pem);
          return keyObj;
        }
      }
      const publicKeyPath = path.join(certsDir, 'public.key');
      if (fs.existsSync(publicKeyPath)) {
        const pem = fs.readFileSync(publicKeyPath, 'utf8');
        return crypto.createPublicKey(pem);
      }
    } catch (_) {}

    throw new Error(
      `RsaKeyManager: Public key for kid "${kid || RsaKeyManager.activeKeyId}" not found.`,
    );
  }

  public static getJwks(): { keys: jose.JWK[] } {
    if (!RsaKeyManager.jwksCache) {
      RsaKeyManager.rebuildJwksCache();
    }
    return RsaKeyManager.jwksCache ?? { keys: [] };
  }

  /**
   * Perform graceful key rotation:
   * 1. Archive current active keypair into certs/archived/
   * 2. Generate a new 4096-bit RSA keypair
   * 3. Set new key as active signing key
   * 4. Expose both keys in JWKS (current + retired)
   */
  public static async rotateKey(
    newKid?: string,
  ): Promise<{ activeKid: string; retiredKid: string }> {
    if (RsaKeyManager.rotationPromise) {
      return RsaKeyManager.rotationPromise;
    }
    RsaKeyManager.rotationPromise = RsaKeyManager.doRotateKey(newKid);
    try {
      return await RsaKeyManager.rotationPromise;
    } finally {
      RsaKeyManager.rotationPromise = null;
    }
  }

  private static async doRotateKey(
    newKid?: string,
  ): Promise<{ activeKid: string; retiredKid: string }> {
    const certsDir = path.resolve(process.cwd(), 'certs');
    const archiveDir = path.join(certsDir, 'archived');
    const privateKeyPath = path.join(certsDir, 'private.key');
    const publicKeyPath = path.join(certsDir, 'public.key');
    const activeKidPath = path.join(certsDir, 'active_kid.txt');

    const retiredKid = RsaKeyManager.activeKeyId;
    const activeKid = newKid || RsaKeyManager.generateKeyId();

    // 1. Archive current key
    if (fs.existsSync(publicKeyPath)) {
      fs.copyFileSync(publicKeyPath, path.join(archiveDir, `${retiredKid}.pub.pem`));
    }
    if (fs.existsSync(privateKeyPath)) {
      fs.copyFileSync(privateKeyPath, path.join(archiveDir, `${retiredKid}.key`));
    }

    // 2. Generate new 4096-bit RSA keypair
    logger.info(`[RsaKeyManager] Rotating key: Generating new active key (${activeKid})...`);
    const { privateKey, publicKey } = crypto.generateKeyPairSync('rsa', {
      modulusLength: 4096,
      publicKeyEncoding: {
        type: 'spki',
        format: 'pem',
      },
      privateKeyEncoding: {
        type: 'pkcs8',
        format: 'pem',
      },
    });

    // 3. Write new files
    fs.writeFileSync(privateKeyPath, privateKey, 'utf8');
    fs.writeFileSync(publicKeyPath, publicKey, 'utf8');
    fs.writeFileSync(activeKidPath, activeKid, 'utf8');

    // 4. Update memory keyStore
    const newPrivateObj = crypto.createPrivateKey(privateKey);
    const newPublicObj = crypto.createPublicKey(publicKey);

    const newJwk = await jose.exportJWK(newPublicObj);
    newJwk.kid = activeKid;
    newJwk.use = 'sig';
    newJwk.alg = 'RS256';

    RsaKeyManager.activeKeyId = activeKid;
    RsaKeyManager.keyStore.set(activeKid, {
      kid: activeKid,
      privateKeyObj: newPrivateObj,
      publicKeyObj: newPublicObj,
      jwk: newJwk,
    });

    // Catat timestamp arsip pada kunci yang dipensiunkan
    const retiredEntry = RsaKeyManager.keyStore.get(retiredKid);
    if (retiredEntry) {
      retiredEntry.archivedAtMs = Date.now();
    }

    RsaKeyManager.rebuildJwksCache();

    logger.info(
      `[RsaKeyManager] Key rotation complete. Active kid: ${activeKid}, Archived kid: ${retiredKid}`,
    );
    return { activeKid, retiredKid };
  }

  private static rebuildJwksCache(): void {
    const now = Date.now();
    const maxAgeMs = RsaKeyManager.jwksMaxAgeDays * 24 * 60 * 60 * 1000;
    const exposedKeys: jose.JWK[] = [];

    for (const entry of RsaKeyManager.keyStore.values()) {
      // 1. Kunci aktif selalu diekspos di endpoint JWKS
      if (entry.kid === RsaKeyManager.activeKeyId) {
        exposedKeys.push(entry.jwk);
        continue;
      }

      // 2. Kunci arsip hanya diekspos jika umurnya <= jwksMaxAgeDays (default: 7 hari)
      const ageMs = entry.archivedAtMs ? now - entry.archivedAtMs : 0;
      if (ageMs <= maxAgeMs) {
        exposedKeys.push(entry.jwk);
      }
    }

    RsaKeyManager.jwksCache = {
      keys: exposedKeys,
    };
  }

  /**
   * Membersihkan file kunci arsip di certs/archived/ yang telah melampaui masa retensi.
   * Kunci yang dihapus otomatis disingkirkan dari in-memory keyStore dan JWKS cache.
   *
   * @param maxAgeDays Batas maksimal umur file arsip dalam hari (default: 30 hari).
   */
  public static pruneArchivedKeys(maxAgeDays: number = 30): {
    prunedKids: string[];
    retainedKids: string[];
  } {
    const certsDir = path.resolve(process.cwd(), 'certs');
    const archiveDir = path.join(certsDir, 'archived');
    if (!fs.existsSync(archiveDir)) {
      return { prunedKids: [], retainedKids: [] };
    }

    const now = Date.now();
    const maxAgeMs = maxAgeDays * 24 * 60 * 60 * 1000;
    const files = fs.readdirSync(archiveDir);
    const prunedKidsSet = new Set<string>();
    const retainedKidsSet = new Set<string>();

    for (const file of files) {
      const filePath = path.join(archiveDir, file);
      try {
        const stats = fs.statSync(filePath);
        const ageMs = now - stats.mtimeMs;
        const kid = file.replace(/\.(pub\.pem|public\.key|key)$/, '');

        if (ageMs > maxAgeMs) {
          fs.unlinkSync(filePath);
          prunedKidsSet.add(kid);
        } else {
          retainedKidsSet.add(kid);
        }
      } catch (err) {
        logger.warn(
          { err },
          `[RsaKeyManager] Gagal memproses/menghapus file arsip ${file}: ${err instanceof Error ? err.message : String(err)}`,
        );
      }
    }

    // Hapus dari in-memory keyStore jika sudah tidak ada file arsip yang tersisa untuk kid tersebut
    for (const prunedKid of prunedKidsSet) {
      if (!retainedKidsSet.has(prunedKid)) {
        RsaKeyManager.keyStore.delete(prunedKid);
      }
    }

    if (prunedKidsSet.size > 0) {
      RsaKeyManager.rebuildJwksCache();
      logger.info(
        `[RsaKeyManager] Pruning selesai: ${prunedKidsSet.size} kunci arsip usang (> ${maxAgeDays} hari) dihapus.`,
      );
    }

    return {
      prunedKids: Array.from(prunedKidsSet),
      retainedKids: Array.from(retainedKidsSet),
    };
  }

  private static fileWatcher: fs.FSWatcher | null = null;
  private static reloadDebounceTimer: NodeJS.Timeout | null = null;

  /**
   * Mengaktifkan live file watcher pada direktori certs/.
   * Bila active_kid.txt atau sertifikat di disk berubah (misal akibat eksekusi CLI),
   * server yang sedang aktif langsung memuat ulang (hot-reload) kunci & cache
   * JWKS secara real-time tanpa perlu restart server sama sekali.
   */
  public static enableFileWatcher(): void {
    if (RsaKeyManager.fileWatcher) return;
    const certsDir = path.resolve(process.cwd(), 'certs');
    if (!fs.existsSync(certsDir)) return;

    try {
      RsaKeyManager.fileWatcher = fs.watch(certsDir, (_eventType, filename) => {
        if (!filename) return;
        if (
          filename === 'active_kid.txt' ||
          filename === 'public.key' ||
          filename.endsWith('.pub.pem')
        ) {
          if (RsaKeyManager.reloadDebounceTimer) {
            clearTimeout(RsaKeyManager.reloadDebounceTimer);
          }
          RsaKeyManager.reloadDebounceTimer = setTimeout(async () => {
            try {
              logger.info(
                '[RsaKeyManager] Mendeteksi rotasi sertifikat di disk. Melakukan live hot-reload kunci...',
              );
              await RsaKeyManager.init();
              logger.info(
                `[RsaKeyManager] Live hot-reload berhasil! Kunci aktif saat ini: ${RsaKeyManager.activeKeyId}`,
              );
            } catch (err) {
              logger.error(
                { err },
                `[RsaKeyManager] Gagal melakukan live hot-reload kunci: ${err instanceof Error ? err.message : String(err)}`,
              );
            }
          }, 300);
        }
      });
      RsaKeyManager.fileWatcher.unref?.();
    } catch (err) {
      logger.warn(
        { err },
        `[RsaKeyManager] Gagal memasang file watcher pada certs/ directory: ${err instanceof Error ? err.message : String(err)}`,
      );
    }
  }

  public static closeFileWatcher(): void {
    if (RsaKeyManager.reloadDebounceTimer) {
      clearTimeout(RsaKeyManager.reloadDebounceTimer);
      RsaKeyManager.reloadDebounceTimer = null;
    }
    if (RsaKeyManager.fileWatcher) {
      RsaKeyManager.fileWatcher.close();
      RsaKeyManager.fileWatcher = null;
    }
  }
}
