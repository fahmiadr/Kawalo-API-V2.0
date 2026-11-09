import { logger } from '@core/utils/logger.service';
import type {
  FloorDeniedFrameDto,
  FloorGrantedFrameDto,
  FloorResetFrameDto,
  FloorRevokedFrameDto,
  FloorStateChangedFrameDto,
  InboundPttControlFrameDto,
  MemberCountUpdatedFrameDto,
} from '@features/ptt/dto/ptt-frame.dto';
import type { PttClusterRelay } from '@features/ptt/infrastructure/ptt-cluster-relay';
import { pttClusterRelay } from '@features/ptt/infrastructure/ptt-cluster-relay';
import {
  type PttRedisRepository,
  pttRedisRepository,
} from '@features/ptt/infrastructure/ptt-redis.repository';
import {
  type PttClientSocket,
  type PttRoomRegistry,
  pttRoomRegistry,
} from '@features/ptt/infrastructure/ptt-room-registry';

export interface PttUserSession {
  sub: string;
  name?: string;
  role?: string | null;
  group_uid?: string | null;
}

export class PttController {
  public static readonly TOT_DURATION_MS = 30000; // 30-second Time-Out Timer

  constructor(
    private readonly redisRepo: PttRedisRepository = pttRedisRepository,
    private readonly roomRegistry: PttRoomRegistry = pttRoomRegistry,
    /** Relay antar-instance (Redis pub/sub). Tanpa relay: perilaku single-instance. */
    private readonly relay?: PttClusterRelay,
  ) {}

  /** Fan-out ke soket lokal, lalu teruskan ke instance lain. */
  private broadcastJson(groupUid: string, payload: unknown, excludeSocketId?: string): void {
    this.roomRegistry.broadcastJson(groupUid, payload, excludeSocketId);
    this.relay?.publishJson(groupUid, payload, excludeSocketId);
  }

  /**
   * Handle new WebSocket connection to /ws/ptt/:groupUid
   */
  public async handleConnection(
    ws: PttClientSocket['ws'] & { id?: string },
    groupUid: string,
    user: PttUserSession,
  ): Promise<void> {
    const socketId = ws.id || `${user.sub}_${Date.now()}_${Math.random().toString(36).slice(2, 8)}`;

    const client: PttClientSocket = {
      id: socketId,
      ws,
      userId: user.sub,
      name: user.name || 'Guard',
      role: (user.role || 'GUARD').toUpperCase(),
    };

    // 1. Register socket in local pod in-memory registry
    this.roomRegistry.register(groupUid, client);
    await this.relay?.join(groupUid);

    // 2. Add user to Redis Set cluster presence
    const memberCount = await this.redisRepo.addMember(groupUid, user.sub);

    // 3. Send current channel state to connecting client
    const currentFloor = await this.redisRepo.getCurrentFloor(groupUid);
    const initialStateFrame: FloorStateChangedFrameDto = {
      type: 'FLOOR_STATE_CHANGED',
      state: currentFloor ? 'TRANSMITTING' : 'IDLE',
      speaker: currentFloor
        ? {
            userId: currentFloor.userId,
            name: currentFloor.name,
            role: currentFloor.role,
          }
        : null,
      memberCount,
    };
    ws.send(JSON.stringify(initialStateFrame));

    // 4. Broadcast updated member count to existing channel members
    const countFrame: MemberCountUpdatedFrameDto = {
      type: 'MEMBER_COUNT_UPDATED',
      memberCount,
    };
    this.broadcastJson(groupUid, countFrame, socketId);

    logger.info(
      `[PTT] Connected: User ${user.sub} (${client.name}/${client.role}) joined group ${groupUid}. Members: ${memberCount}`,
    );
  }

  /**
   * Handle WebSocket disconnection
   */
  public async handleDisconnection(
    socketId: string,
    groupUid: string,
    user: PttUserSession,
  ): Promise<void> {
    const wasActiveSpeaker = this.roomRegistry.isCurrentActiveSpeaker(groupUid, socketId);

    // 1. Unregister from in-memory registry
    const unregistered = this.roomRegistry.unregister(groupUid, socketId);
    if (unregistered?.remainingCount === 0) {
      await this.relay?.leave(groupUid);
    }

    // 2. Remove user from Redis Set presence
    const memberCount = await this.redisRepo.removeMember(groupUid, user.sub);

    // 3. If disconnecting client was the active speaker, immediately release floor
    if (wasActiveSpeaker) {
      this.roomRegistry.clearTotTimer(groupUid);
      this.roomRegistry.setActiveSpeaker(groupUid, null);
      await this.redisRepo.releaseFloor(groupUid, user.sub);

      const idleFrame: FloorStateChangedFrameDto = {
        type: 'FLOOR_STATE_CHANGED',
        state: 'IDLE',
        speaker: null,
        memberCount,
      };
      this.broadcastJson(groupUid, idleFrame);
      logger.info(`[PTT] Active speaker disconnected. Floor reset to IDLE in group ${groupUid}.`);
    } else {
      // Broadcast updated member count to remaining subscribers
      const countFrame: MemberCountUpdatedFrameDto = {
        type: 'MEMBER_COUNT_UPDATED',
        memberCount,
      };
      this.broadcastJson(groupUid, countFrame);
    }

    logger.info(
      `[PTT] Disconnected: User ${user.sub} left group ${groupUid}. Remaining members: ${memberCount}`,
    );
  }

  /**
   * Handle inbound JSON control frames (FLOOR_REQUEST, FLOOR_RELEASED)
   */
  public async handleControlFrame(
    ws: PttClientSocket['ws'] & { id?: string },
    groupUid: string,
    user: PttUserSession,
    frame: InboundPttControlFrameDto,
  ): Promise<void> {
    const socketId = ws.id || '';
    const userRole = (user.role || 'GUARD').toUpperCase();
    const userName = user.name || 'Guard';

    switch (frame.type) {
      case 'FLOOR_REQUEST': {
        const acquireResult = await this.redisRepo.acquireFloor(
          groupUid,
          {
            userId: user.sub,
            name: userName,
            role: userRole,
            socketId,
          },
          5, // 5000ms TTL (robust against network jitter and brief speech pauses)
        );

        if (acquireResult.granted) {
          // Set active speaker in room registry
          this.roomRegistry.setActiveSpeaker(groupUid, {
            socketId,
            userId: user.sub,
            name: userName,
            role: userRole,
            burstStartMs: Date.now(),
          });

          // Arm 30-second Time-Out Timer (TOT)
          const totTimer = setTimeout(() => {
            void this.handleTotExpiry(groupUid, user.sub, socketId);
          }, PttController.TOT_DURATION_MS);
          this.roomRegistry.setTotTimer(groupUid, totTimer);

          // 1. Send FLOOR_GRANTED to requester
          const grantedFrame: FloorGrantedFrameDto = {
            type: 'FLOOR_GRANTED',
            requestId: frame.requestId,
            leaseTtlMs: 5000,
          };
          ws.send(JSON.stringify(grantedFrame));

          // 2. If Admin Preemption occurred, notify previous speaker and purge listeners
          if (acquireResult.preempted && acquireResult.previousSpeaker) {
            const prevSpeaker = acquireResult.previousSpeaker;
            const prevClient = this.roomRegistry.getClientByUserId(groupUid, prevSpeaker.userId);

            const revokedFrame: FloorRevokedFrameDto = {
              type: 'FLOOR_REVOKED',
              reason: 'ADMIN_OVERRIDE',
              preemptedBy: userName,
            };
            // Pembicara sebelumnya bisa tersambung ke instance lain
            this.relay?.publishPreempted(groupUid, prevSpeaker.userId, revokedFrame);

            if (prevClient) {
              try {
                prevClient.ws.send(JSON.stringify(revokedFrame));
              } catch {
                // Ignore socket write failures on closed sockets
              }
            }

            // Broadcast FLOOR_RESET (PREEMPTION_FLUSH) to purge listener jitter buffers
            const resetFrame: FloorResetFrameDto = {
              type: 'FLOOR_RESET',
              reason: 'PREEMPTION_FLUSH',
            };
            this.broadcastJson(groupUid, resetFrame, socketId);

            logger.info(
              `[PTT] Admin ${user.sub} (${userName}) preempted Guard ${prevSpeaker.userId} in group ${groupUid}`,
            );
          }

          // 3. Broadcast FLOOR_STATE_CHANGED (TRANSMITTING) to all subscribers
          const memberCount = await this.redisRepo.getMemberCount(groupUid);
          const stateFrame: FloorStateChangedFrameDto = {
            type: 'FLOOR_STATE_CHANGED',
            state: 'TRANSMITTING',
            speaker: {
              userId: user.sub,
              name: userName,
              role: userRole,
            },
            memberCount,
          };
          this.broadcastJson(groupUid, stateFrame);
        } else {
          // Floor Denied
          const deniedFrame: FloorDeniedFrameDto = {
            type: 'FLOOR_DENIED',
            requestId: frame.requestId,
            reason:
              userRole === 'ADMIN'
                ? 'CHANNEL_BUSY'
                : acquireResult.currentSpeaker?.role?.toUpperCase() === 'ADMIN'
                  ? 'INSUFFICIENT_PRIORITY'
                  : 'CHANNEL_BUSY',
            currentSpeaker: acquireResult.currentSpeaker
              ? {
                  userId: acquireResult.currentSpeaker.userId,
                  name: acquireResult.currentSpeaker.name,
                  role: acquireResult.currentSpeaker.role,
                }
              : undefined,
          };
          ws.send(JSON.stringify(deniedFrame));
          logger.info(
            `[PTT] Floor denied to user ${user.sub} in group ${groupUid}: ${deniedFrame.reason}`,
          );
        }
        break;
      }

      case 'FLOOR_RELEASED': {
        // Only active speaker can release the floor (checked via socketId or userId)
        if (!this.roomRegistry.isCurrentActiveSpeaker(groupUid, socketId, user.sub)) {
          return;
        }

        // Disarm TOT
        this.roomRegistry.clearTotTimer(groupUid);
        this.roomRegistry.setActiveSpeaker(groupUid, null);

        // Delete Redis floor key
        await this.redisRepo.releaseFloor(groupUid, user.sub);

        // Broadcast FLOOR_STATE_CHANGED (IDLE) to all room members
        const memberCount = await this.redisRepo.getMemberCount(groupUid);
        const idleFrame: FloorStateChangedFrameDto = {
          type: 'FLOOR_STATE_CHANGED',
          state: 'IDLE',
          speaker: null,
          memberCount,
        };
        this.broadcastJson(groupUid, idleFrame);

        logger.info(
          `[PTT] Floor released by user ${user.sub} in group ${groupUid}. State is now IDLE.`,
        );
        break;
      }

      default:
        break;
    }
  }

  /**
   * Handle inbound binary audio frame (Opus packet with 7-byte header).
   * Strictly enforces:
   * 1. In-flight packet dropping for preempted or non-active speakers.
   * 2. 1Hz debounced Redis lease renewal.
   * 3. TCP backpressure check for subscribers.
   */
  public handleBinaryAudioFrame(
    socketId: string,
    groupUid: string,
    user: PttUserSession,
    binaryBuffer: Buffer | Uint8Array,
  ): void {
    // 1. In-Flight Dropping Guard: Drop packets if sender is NOT the current active speaker
    if (!this.roomRegistry.isCurrentActiveSpeaker(groupUid, socketId, user.sub)) {
      return; // Discard late in-flight packet after preemption or unauthorized audio
    }

    // 2. Debounced Redis Lease Renewal (1Hz throttling):
    // Prevents 50Hz audio packets from flooding Redis with PEXPIRE commands
    const now = Date.now();
    const lastRenew = this.roomRegistry.getLastLeaseRenewal(groupUid);
    if (now - lastRenew >= 1000) {
      this.roomRegistry.setLastLeaseRenewal(groupUid, now);
      this.redisRepo.extendFloorLease(groupUid, user.sub, 5).catch((err) => {
        logger.warn({ err }, `[PTT] Failed to extend floor lease for user ${user.sub}`);
      });
    }

    // 3. Fast In-Memory Fan-Out to subscribers with TCP Backpressure check
    this.roomRegistry.broadcastBinary(groupUid, binaryBuffer, socketId);
    this.relay?.publishBinary(groupUid, binaryBuffer);
  }

  /**
   * Handle 30-second Time-Out Timer (TOT) expiration.
   * Forcibly revokes floor from mechanically pinned or stuck buttons.
   */
  private async handleTotExpiry(groupUid: string, userId: string, socketId: string): Promise<void> {
    logger.warn(`[PTT] TOT Expired: 30s limit exceeded for user ${userId} in group ${groupUid}.`);

    // 1. Notify speaker that floor was revoked
    const client = this.roomRegistry.getClient(groupUid, socketId);
    if (client) {
      const revokedFrame: FloorRevokedFrameDto = {
        type: 'FLOOR_REVOKED',
        reason: 'TIME_OUT_TIMER_EXPIRED',
      };
      try {
        client.ws.send(JSON.stringify(revokedFrame));
      } catch {
        // Ignore
      }
    }

    // 2. Clear active speaker in registry and Redis
    this.roomRegistry.setActiveSpeaker(groupUid, null);
    await this.redisRepo.releaseFloor(groupUid, userId);

    // 3. Broadcast FLOOR_STATE_CHANGED (IDLE) to all room members
    const memberCount = await this.redisRepo.getMemberCount(groupUid);
    const idleFrame: FloorStateChangedFrameDto = {
      type: 'FLOOR_STATE_CHANGED',
      state: 'IDLE',
      speaker: null,
      memberCount,
    };
    this.broadcastJson(groupUid, idleFrame);
  }
}

export const pttController = new PttController(
  pttRedisRepository,
  pttRoomRegistry,
  pttClusterRelay,
);
