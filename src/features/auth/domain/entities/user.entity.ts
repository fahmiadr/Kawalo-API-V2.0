export interface UserGroupAccessProps {
  groupUid: string;
  groupName?: string | null;
  groupOwner?: string | null;
  category?: string | null;
  status?: string | null;
  groupStatus?: string | null;
  groupLogo?: string | null;
  allowAdmin?: boolean;
  allowPatrol?: boolean;
  allowMonPatrol?: boolean;
  features?: string[];
}

export class UserGroupAccessEntity {
  public readonly groupUid: string;
  public readonly groupName: string | null;
  public readonly groupOwner: string | null;
  public readonly category: string | null;
  public readonly status: string | null;
  public readonly groupStatus: string | null;
  public readonly groupLogo: string | null;
  public readonly allowAdmin: boolean;
  public readonly allowPatrol: boolean;
  public readonly allowMonPatrol: boolean;
  public readonly features: string[];

  constructor(props: UserGroupAccessProps) {
    this.groupUid = props.groupUid;
    this.groupName = props.groupName ?? null;
    this.groupOwner = props.groupOwner ?? null;
    this.category = props.category ?? null;
    this.status = props.status ?? null;
    this.groupStatus = props.groupStatus ?? null;
    this.groupLogo = props.groupLogo ?? null;
    this.allowAdmin = props.allowAdmin ?? false;
    this.allowPatrol = props.allowPatrol ?? false;
    this.allowMonPatrol = props.allowMonPatrol ?? false;
    this.features = props.features ?? [];
  }
}

export interface UserProps {
  id: string;
  email: string;
  name: string;
  passwordHash: string;
  avatarUrl?: string | null;
  activeGroup?: UserGroupAccessEntity | null;
  groups?: UserGroupAccessEntity[];
}

export class UserEntity {
  public readonly id: string;
  public readonly email: string;
  public readonly name: string;
  public readonly passwordHash: string;
  public readonly avatarUrl: string | null;
  public readonly activeGroup: UserGroupAccessEntity | null;
  public readonly groups: UserGroupAccessEntity[];

  constructor(props: UserProps) {
    this.id = props.id;
    this.email = props.email;
    this.name = props.name;
    this.passwordHash = props.passwordHash;
    this.avatarUrl = props.avatarUrl ?? null;
    this.activeGroup = props.activeGroup ?? null;
    this.groups = props.groups ?? [];
  }

  public get role(): string | null {
    return this.activeGroup?.category ?? null;
  }

  public get allowAdmin(): boolean {
    return this.activeGroup?.allowAdmin ?? false;
  }

  public get allowPatrol(): boolean {
    return this.activeGroup?.allowPatrol ?? false;
  }

  public get allowMonPatrol(): boolean {
    return this.activeGroup?.allowMonPatrol ?? false;
  }
}
