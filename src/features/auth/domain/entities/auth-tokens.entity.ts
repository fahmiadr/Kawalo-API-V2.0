export interface AuthTokensProps {
  accessToken: string;
  refreshToken: string;
  tokenType: string;
  expiresIn: number;
}

export class AuthTokensEntity {
  public readonly accessToken: string;
  public readonly refreshToken: string;
  public readonly tokenType: string;
  public readonly expiresIn: number;

  constructor(props: AuthTokensProps) {
    this.accessToken = props.accessToken;
    this.refreshToken = props.refreshToken;
    this.tokenType = props.tokenType;
    this.expiresIn = props.expiresIn;
  }
}

export interface RefreshTokenProps {
  userId: string;
  tokenHash: string;
  familyId: string;
  expiresAt: Date;
  activeGroupUid?: string | null;
  createdAt?: Date;
}

export class RefreshTokenEntity {
  public readonly userId: string;
  public readonly tokenHash: string;
  public readonly familyId: string;
  public readonly expiresAt: Date;
  public readonly activeGroupUid: string | null;
  public readonly createdAt: Date;

  constructor(props: RefreshTokenProps) {
    this.userId = props.userId;
    this.tokenHash = props.tokenHash;
    this.familyId = props.familyId;
    this.expiresAt = props.expiresAt;
    this.activeGroupUid = props.activeGroupUid ?? null;
    this.createdAt = props.createdAt ?? new Date();
  }
}
