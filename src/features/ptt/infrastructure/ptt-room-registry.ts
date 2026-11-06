export interface PttClientSocket {
  id: string;
  ws: {
    send(data: unknown): void;
    close?(code?: number, reason?: string): void;
    raw?: { bufferedAmount?: number };
    bufferedAmount?: number;
    data?: unknown;
  };
  userId: string;
  name: string;
  role: string;
}

export interface PttActiveSpeakerInfo {
  socketId: string;
  userId: string;
  name: string;
  role: string;
  burstStartMs: number;
}

export class PttRoomRegistry {
  public static readonly DEFAULT_MAX_BUFFERED_AMOUNT = 5 * 1024; // 5KB (~100ms / 5 audio frames)

  // Maps groupUid -> Map<socketId, PttClientSocket>
  private readonly rooms = new Map<string, Map<string, PttClientSocket>>();

  // Active speaker per group (for zero-latency in-flight audio frame dropping on preemption)
  private readonly activeSpeakers = new Map<string, PttActiveSpeakerInfo>();

  // Active Time-Out Timers per group
  private readonly totTimers = new Map<string, NodeJS.Timeout>();

  // Last Redis lease renewal timestamp per group (for 1Hz debounced renewals)
  private readonly lastLeaseRenewals = new Map<string, number>();

  public register(groupUid: string, client: PttClientSocket): void {
    let room = this.rooms.get(groupUid);
    if (!room) {
      room = new Map();
      this.rooms.set(groupUid, room);
    }
    room.set(client.id, client);
  }

  public unregister(
    groupUid: string,
    socketId: string,
  ): { remainingCount: number; removedClient?: PttClientSocket } {
    const room = this.rooms.get(groupUid);
    if (!room) {
      return { remainingCount: 0 };
    }

    const removedClient = room.get(socketId);
    room.delete(socketId);

    const remainingCount = room.size;
    if (remainingCount === 0) {
      this.rooms.delete(groupUid);
      this.clearTotTimer(groupUid);
      this.activeSpeakers.delete(groupUid);
      this.lastLeaseRenewals.delete(groupUid);
    }

    return { remainingCount, removedClient };
  }

  public getClients(groupUid: string): PttClientSocket[] {
    const room = this.rooms.get(groupUid);
    return room ? Array.from(room.values()) : [];
  }

  public getClient(groupUid: string, socketId: string): PttClientSocket | undefined {
    return this.rooms.get(groupUid)?.get(socketId);
  }

  public getClientByUserId(groupUid: string, userId: string): PttClientSocket | undefined {
    const clients = this.getClients(groupUid);
    return clients.find((c) => c.userId === userId);
  }

  public getClientCount(groupUid: string): number {
    return this.rooms.get(groupUid)?.size ?? 0;
  }

  /* ---------------- Broadcast Operations ---------------- */

  /**
   * Broadcast JSON control frame to all connected squad members in the group.
   */
  public broadcastJson(groupUid: string, payload: unknown, excludeSocketId?: string): void {
    const clients = this.getClients(groupUid);
    const jsonStr = typeof payload === 'string' ? payload : JSON.stringify(payload);

    for (const client of clients) {
      if (excludeSocketId && client.id === excludeSocketId) {
        continue;
      }
      try {
        client.ws.send(jsonStr);
      } catch {
        // Socket errors handled in socket lifecycle
      }
    }
  }

  /**
   * Broadcast binary audio frame with strict TCP Backpressure Guard.
   * If client.bufferedAmount > 5KB, packet is dropped to maintain sub-200ms real-time audio.
   */
  public broadcastBinary(
    groupUid: string,
    binaryFrame: Buffer | Uint8Array,
    excludeSocketId?: string,
    maxBufferedAmount = PttRoomRegistry.DEFAULT_MAX_BUFFERED_AMOUNT,
  ): { sentCount: number; droppedCount: number } {
    const clients = this.getClients(groupUid);
    let sentCount = 0;
    let droppedCount = 0;

    for (const client of clients) {
      if (excludeSocketId && client.id === excludeSocketId) {
        continue;
      }

      // Check TCP write buffer backpressure
      const bufferAmount = client.ws.raw?.bufferedAmount ?? client.ws.bufferedAmount ?? 0;
      if (bufferAmount > maxBufferedAmount) {
        droppedCount++;
        continue; // Drop packet to preserve conversational freshness
      }

      try {
        client.ws.send(binaryFrame);
        sentCount++;
      } catch {
        droppedCount++;
      }
    }

    return { sentCount, droppedCount };
  }

  /* ---------------- Active Speaker & Preemption In-Flight Guards ---------------- */

  public setActiveSpeaker(groupUid: string, speaker: PttActiveSpeakerInfo | null): void {
    if (speaker) {
      this.activeSpeakers.set(groupUid, speaker);
    } else {
      this.activeSpeakers.delete(groupUid);
      this.clearTotTimer(groupUid);
      this.lastLeaseRenewals.delete(groupUid);
    }
  }

  public getActiveSpeaker(groupUid: string): PttActiveSpeakerInfo | null {
    return this.activeSpeakers.get(groupUid) ?? null;
  }

  public isCurrentActiveSpeaker(groupUid: string, socketId: string, userId?: string): boolean {
    const current = this.activeSpeakers.get(groupUid);
    if (!current) return false;
    if (current.socketId === socketId) return true;
    if (userId !== undefined && current.userId === userId) return true;
    return false;
  }

  /* ---------------- Time-Out Timer (30s TOT) ---------------- */

  public setTotTimer(groupUid: string, timer: NodeJS.Timeout): void {
    this.clearTotTimer(groupUid);
    this.totTimers.set(groupUid, timer);
  }

  public clearTotTimer(groupUid: string): void {
    const existing = this.totTimers.get(groupUid);
    if (existing) {
      clearTimeout(existing);
      this.totTimers.delete(groupUid);
    }
  }

  /* ---------------- Lease Renewal Throttling (1Hz) ---------------- */

  public getLastLeaseRenewal(groupUid: string): number {
    return this.lastLeaseRenewals.get(groupUid) ?? 0;
  }

  public setLastLeaseRenewal(groupUid: string, timestampMs: number): void {
    this.lastLeaseRenewals.set(groupUid, timestampMs);
  }

  /* ---------------- Reset / Cleanup (for Testing) ---------------- */

  public clearAll(): void {
    for (const timer of this.totTimers.values()) {
      clearTimeout(timer);
    }
    this.totTimers.clear();
    this.rooms.clear();
    this.activeSpeakers.clear();
    this.lastLeaseRenewals.clear();
  }
}

export const pttRoomRegistry = new PttRoomRegistry();
