export type PttFloorState = 'IDLE' | 'TRANSMITTING';

export interface PttFloorSpeaker {
  userId: string;
  name: string;
  role: string;
  socketId?: string;
}

export interface PttFloorProps {
  groupUid: string;
  state: PttFloorState;
  speaker: PttFloorSpeaker | null;
  grantedAtMs: number | null;
  leaseExpiresAtMs: number | null;
  burstStartMs: number | null;
}

export class PttFloor {
  public static readonly DEFAULT_LEASE_TTL_MS = 2000;
  public static readonly MAX_TRANSMISSION_DURATION_MS = 30000; // 30s Time-Out Timer (TOT)

  constructor(private readonly props: PttFloorProps) {}

  public get groupUid(): string {
    return this.props.groupUid;
  }

  public get state(): PttFloorState {
    return this.props.state;
  }

  public get speaker(): PttFloorSpeaker | null {
    return this.props.speaker ? { ...this.props.speaker } : null;
  }

  public get grantedAtMs(): number | null {
    return this.props.grantedAtMs;
  }

  public get leaseExpiresAtMs(): number | null {
    return this.props.leaseExpiresAtMs;
  }

  public get burstStartMs(): number | null {
    return this.props.burstStartMs;
  }

  public static createIdle(groupUid: string): PttFloor {
    return new PttFloor({
      groupUid,
      state: 'IDLE',
      speaker: null,
      grantedAtMs: null,
      leaseExpiresAtMs: null,
      burstStartMs: null,
    });
  }

  public static createTransmitting(
    groupUid: string,
    speaker: PttFloorSpeaker,
    nowMs: number = Date.now(),
    leaseTtlMs: number = PttFloor.DEFAULT_LEASE_TTL_MS,
  ): PttFloor {
    return new PttFloor({
      groupUid,
      state: 'TRANSMITTING',
      speaker: { ...speaker },
      grantedAtMs: nowMs,
      leaseExpiresAtMs: nowMs + leaseTtlMs,
      burstStartMs: nowMs,
    });
  }

  public isOccupied(): boolean {
    return this.props.state === 'TRANSMITTING' && this.props.speaker !== null;
  }

  public isExpired(nowMs: number = Date.now()): boolean {
    if (!this.isOccupied() || this.props.leaseExpiresAtMs === null) {
      return false;
    }
    return nowMs >= this.props.leaseExpiresAtMs;
  }

  public isTotExceeded(
    nowMs: number = Date.now(),
    maxDurationMs: number = PttFloor.MAX_TRANSMISSION_DURATION_MS,
  ): boolean {
    if (!this.isOccupied() || this.props.burstStartMs === null) {
      return false;
    }
    return nowMs - this.props.burstStartMs >= maxDurationMs;
  }

  /**
   * Deterministic Two-Tier Floor Control:
   * Admin can preempt active Guard transmission.
   * Equal tier (Guard vs Guard or Admin vs Admin) cannot preempt (FIFO).
   */
  public canPreempt(requesterRole: string): boolean {
    if (!this.isOccupied() || !this.props.speaker) {
      return true;
    }

    const normalizedRequester = requesterRole.toUpperCase();
    const normalizedSpeaker = this.props.speaker.role.toUpperCase();

    // Admin can preempt regular Guards
    return normalizedRequester === 'ADMIN' && normalizedSpeaker !== 'ADMIN';
  }

  public renewLease(
    nowMs: number = Date.now(),
    leaseTtlMs: number = PttFloor.DEFAULT_LEASE_TTL_MS,
  ): PttFloor {
    if (!this.isOccupied()) {
      return this;
    }

    return new PttFloor({
      ...this.props,
      leaseExpiresAtMs: nowMs + leaseTtlMs,
    });
  }

  public release(): PttFloor {
    return PttFloor.createIdle(this.props.groupUid);
  }

  public toJSON(): Record<string, unknown> {
    return {
      groupUid: this.props.groupUid,
      state: this.props.state,
      speaker: this.props.speaker,
      grantedAtMs: this.props.grantedAtMs,
      leaseExpiresAtMs: this.props.leaseExpiresAtMs,
      burstStartMs: this.props.burstStartMs,
    };
  }
}
