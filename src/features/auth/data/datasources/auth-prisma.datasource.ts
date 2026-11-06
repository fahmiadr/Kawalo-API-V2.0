import { prisma } from '@core/database/prisma.service';
import { cleanInteger, cleanString } from '@core/utils/clean-data.util';
import { UserEntity, UserGroupAccessEntity } from '@features/auth/domain/entities/user.entity';
import type { PrismaClient } from '@generated/prisma/client';

export class AuthPrismaDataSource {
  constructor(private readonly db: PrismaClient = prisma) {}

  /**
   * Cari personil berdasarkan email di `tkuser` dan resolve relasi grup ke `tkusergroup` dan `tkgroup`.
   */
  public async findByEmail(
    email: string,
    requestedGroupUid?: string | null,
  ): Promise<UserEntity | null> {
    if (!email) {
      return null;
    }

    // 2. Explicit select data pada tkuser (hanya kolom yang dipetakan ke UserEntity)
    const tkUser = await this.db.tkuser.findUnique({
      where: { UserEmail: email },
      select: {
        UserId: true,
        UserEmail: true,
        UserName: true,
        Password: true,
        UserProfilePic: true,
      },
    });

    if (!tkUser) {
      return null;
    }

    // 3. Sanitasi seluruh atribut mentah tkuser ke variabel lokal (Zero Double-Cleaning & Anti-Hallucination)
    const cleanUser = this.sanitizeUserRecord(tkUser);
    if (!cleanUser) {
      return null;
    }

    // 4. Resolve keanggotaan grup secara terisolasi via Safe Map Lookup
    const groups = await this.resolveUserGroups(cleanUser.email);

    // 5. Konstruksi UserEntity dengan data yang 100% bersih, aman & terverifikasi
    return this.buildUserEntity(cleanUser, groups, requestedGroupUid);
  }

  /**
   * Cari personil berdasarkan id numerik (`UserId`) dan resolve relasi grup.
   */
  public async findById(id: string, requestedGroupUid?: string | null): Promise<UserEntity | null> {
    if (!id) {
      return null;
    }

    const numericId = Number(id);
    if (!Number.isInteger(numericId) || numericId <= 0) {
      return null;
    }

    // 2. Explicit select data pada tkuser (hanya kolom yang dipetakan ke UserEntity)
    const tkUser = await this.db.tkuser.findUnique({
      where: { UserId: numericId },
      select: {
        UserId: true,
        UserEmail: true,
        UserName: true,
        Password: true,
        UserProfilePic: true,
      },
    });

    if (!tkUser) {
      return null;
    }

    // 3. Sanitasi seluruh atribut mentah tkuser ke variabel lokal (Zero Double-Cleaning & Anti-Hallucination)
    const cleanUser = this.sanitizeUserRecord(tkUser);
    if (!cleanUser) {
      return null;
    }

    // 4. Resolve keanggotaan grup secara terisolasi via Safe Map Lookup
    const groups = await this.resolveUserGroups(cleanUser.email);

    // 5. Konstruksi UserEntity dengan data yang 100% bersih, aman & terverifikasi
    return this.buildUserEntity(cleanUser, groups, requestedGroupUid);
  }

  /**
   * Sanitasi data mentah record `tkuser` ke dalam variabel lokal dengan validasi integritas ketat.
   * Mengembalikan null jika atribut esensial (email, nama, password) hilang/rusak (Zero Data Hallucination).
   */
  private sanitizeUserRecord(raw: {
    UserId: number;
    UserEmail: string;
    UserName: string;
    Password: string;
    UserProfilePic: string | null;
  }): {
    id: string;
    email: string;
    name: string;
    passwordHash: string;
    avatarUrl: string | null;
  } | null {
    const cleanId = cleanInteger(raw.UserId);
    const email = cleanString(raw.UserEmail);
    const name = cleanString(raw.UserName);
    const passwordHash = cleanString(raw.Password);
    const avatarUrl = cleanString(raw.UserProfilePic);

    // Guard integritas: Mencegah zero data hallucination / synthesis objek korup
    if (cleanId === null || !email || !name || !passwordHash) {
      return null;
    }

    return {
      id: String(cleanId),
      email,
      name,
      passwordHash,
      avatarUrl,
    };
  }

  /**
   * Konstruksi `UserEntity` menggunakan data user yang sudah terverifikasi bersih (Zero Double-Cleaning).
   */
  private buildUserEntity(
    user: {
      id: string;
      email: string;
      name: string;
      passwordHash: string;
      avatarUrl: string | null;
    },
    groups: UserGroupAccessEntity[] = [],
    requestedGroupUid?: string | null,
  ): UserEntity {
    const cleanRequestedGroupUid = cleanString(requestedGroupUid);
    const activeGroup = cleanRequestedGroupUid
      ? (groups.find((g) => g.groupUid === cleanRequestedGroupUid) ?? null)
      : (groups.find((g) => g.status === 'JOINED') ?? null);

    return new UserEntity({
      id: user.id,
      email: user.email,
      name: user.name,
      passwordHash: user.passwordHash,
      avatarUrl: user.avatarUrl,
      activeGroup,
      groups,
    });
  }

  /**
   * Menggabungkan relasi keanggotaan grup (`tkusergroup`) dengan detail grup (`tkgroup`) via Safe Map Lookup.
   */
  private async resolveUserGroups(email: string): Promise<UserGroupAccessEntity[]> {
    // 1. Explicit select pada tkusergroup (hanya kolom yang dipetakan ke UserGroupAccessEntity)
    const userGroups = await this.db.tkusergroup.findMany({
      where: {
        Email: email,
      },
      select: {
        GroupUID: true,
        AllowAdmin: true,
        AllowPatrol: true,
        AllowMonPatrol: true,
        Category: true,
        Status: true,
      },
    });

    if (userGroups.length === 0) {
      return [];
    }

    // 2. Sanitasi dan deduplikasi GroupUID untuk mencegah query bloat dan data poisoning
    const groupUids = Array.from(
      new Set(
        userGroups
          .map((ug) => cleanString(ug.GroupUID))
          .filter((uid): uid is string => Boolean(uid)),
      ),
    );

    if (groupUids.length === 0) {
      return [];
    }

    // 3. Explicit select data pada tkgroup (hanya kolom yang dipetakan ke UserGroupAccessEntity)
    const groups = await this.db.tkgroup.findMany({
      where: {
        GroupUID: { in: groupUids },
      },
      select: {
        GroupUID: true,
        GroupName: true,
        GroupOwner: true,
        GroupLogo: true,
        Status: true,
        Features: true,
      },
    });

    // 4. Safe In-Memory Map Lookup
    const groupMap = new Map(
      groups
        .map((g) => [cleanString(g.GroupUID), g] as const)
        .filter((entry): entry is [string, (typeof groups)[number]] => Boolean(entry[0])),
    );

    // 5. Mapping ke UserGroupAccessEntity menggunakan Pipeline 5-Langkah ACL (100% Pra-Sanitasi & Pure Nullable)
    return userGroups
      .map((ug) => {
        // 1. Sanitasi seluruh atribut mentah dari record keanggotaan (tkusergroup)
        const groupUid = cleanString(ug.GroupUID);
        if (!groupUid) {
          return null;
        }
        const category = cleanString(ug.Category)?.toUpperCase();
        const status = cleanString(ug.Status);
        const rawAllowAdmin = cleanString(ug.AllowAdmin);
        const rawAllowPatrol = cleanString(ug.AllowPatrol);
        const rawAllowMonPatrol = cleanString(ug.AllowMonPatrol);

        // 2. Safe Map Lookup data relasional grup (tkgroup)
        const groupDetail = groupMap.get(groupUid) ?? null;

        // 3. Sanitasi seluruh atribut mentah dari record relasi grup (tkgroup)
        const groupName = cleanString(groupDetail?.GroupName);
        const groupOwner = cleanString(groupDetail?.GroupOwner);
        const groupLogo = cleanString(groupDetail?.GroupLogo);
        const groupStatus = cleanString(groupDetail?.Status);
        const rawFeatures = cleanString(groupDetail?.Features);

        // 4. Evaluasi logika domain & parsing flags anti-corruption
        const allowAdmin = rawAllowAdmin?.toUpperCase() === 'YES';
        const allowPatrol = rawAllowPatrol?.toUpperCase() === 'YES';
        const allowMonPatrol = rawAllowMonPatrol?.toUpperCase() === 'YES';
        const features = rawFeatures
          ? rawFeatures
              .split(';')
              .map((f) => cleanString(f))
              .filter((f): f is string => Boolean(f))
          : [];

        // 5. Konstruksi UserGroupAccessEntity murni menggunakan data yang 100% sudah bersih & nullable
        return new UserGroupAccessEntity({
          groupUid,
          groupName,
          groupOwner,
          category,
          status,
          groupStatus,
          groupLogo,
          allowAdmin,
          allowPatrol,
          allowMonPatrol,
          features,
        });
      })
      .filter((access): access is UserGroupAccessEntity => access !== null);
  }
}
