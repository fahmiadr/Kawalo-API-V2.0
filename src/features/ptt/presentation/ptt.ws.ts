import { nodeAdapter } from '@core/adapter/node.adapter';
import { SessionVersionService } from '@core/security/session-version.service';
import { TokenService } from '@core/security/token.service';
import type { JwtUserPayload } from '@core/types/jwt-payload.types';
import { logger } from '@core/utils/logger.service';
import type { InboundPttControlFrameDto } from '@features/ptt/dto/ptt-frame.dto';
import { pttController } from '@features/ptt/presentation/ptt.controller';
import { Elysia, t } from 'elysia';

export interface PttWsContextData {
  user?: JwtUserPayload;
  groupUid: string;
}

export const pttWsRoute = new Elysia({ name: 'ptt-ws', adapter: nodeAdapter }).ws('/ws/ptt/:groupUid', {
  params: t.Object({
    groupUid: t.String({ minLength: 1 }),
  }),
  query: t.Object({
    token: t.Optional(t.String()),
  }),
  /**
   * Handshake Authentication & Group Authorization Guard
   */
  beforeHandle: async ({ query, headers, params, status }) => {
    const rawToken =
      query.token ||
      headers.authorization?.replace(/^Bearer\s+/i, '') ||
      headers['sec-websocket-protocol'];

    if (!rawToken) {
      return status(401, 'Unauthorized: Missing access token');
    }

    // 1. Verify RS256 JWT cryptographic signature
    const userPayload = await TokenService.verifyAccessToken(rawToken);
    if (!userPayload?.sub) {
      return status(401, 'Unauthorized: Invalid access token signature');
    }

    // 2. Validate session version against Redis
    const isSessionValid = await SessionVersionService.validateOrRehydrate(
      userPayload.sub,
      userPayload.session_ver,
    );
    if (!isSessionValid) {
      return status(401, 'Unauthorized: Session revoked or expired');
    }

    // 3. Verify organizational group tenancy
    const userRole = userPayload.role?.toUpperCase();
    if (
      userRole !== 'SUPER_ADMIN' &&
      userPayload.group_uid &&
      userPayload.group_uid !== params.groupUid
    ) {
      return status(403, 'Forbidden: User does not belong to this squad group');
    }
  },

  /**
   * WebSocket Client Lifecycle
   */
  open: async (ws) => {
    try {
      const rawToken =
        ws.data?.query?.token ||
        ws.data?.headers?.authorization?.replace(/^Bearer\s+/i, '') ||
        ws.data?.headers?.['sec-websocket-protocol'];

      const user = await TokenService.verifyAccessToken(rawToken || '');
      if (!user?.sub) {
        ws.close(4001, 'Unauthorized');
        return;
      }

      // Attach verified user context to socket
      (ws.data as unknown as { user: JwtUserPayload }).user = user;

      const groupUid = ws.data?.params?.groupUid || user.group_uid || '';

      // Assign unique socket id if not already provided by adapter
      const socketId =
        ws.id || `${user.sub}_${Date.now()}_${Math.random().toString(36).slice(2, 7)}`;
      const socket = {
        id: socketId,
        send: (data: unknown) => ws.send(data),
      };

      // Store socket object on ws.data for reuse in message and close lifecycles
      (ws.data as unknown as Record<string, unknown>).socket = socket;

      await pttController.handleConnection(socket, groupUid, {
        sub: user.sub,
        name: user.name,
        role: user.role,
        group_uid: user.group_uid,
      });
    } catch (err) {
      logger.error({ err }, '[PTT] Error during WebSocket open lifecycle');
    }
  },

  message: async (ws, message) => {
    try {
      const socket =
        ((ws.data as unknown as Record<string, unknown>)?.socket as { id: string; send: (data: unknown) => void } | undefined) ||
        (ws as unknown as { id: string; send: (data: unknown) => void });
      const user = (ws.data as unknown as { user?: JwtUserPayload })?.user;
      if (!user?.sub) {
        return;
      }

      const groupUid = ws.data?.params?.groupUid || user.group_uid || '';
      const userSession = {
        sub: user.sub,
        name: user.name,
        role: user.role,
        group_uid: user.group_uid,
      };

      // 1. Binary Audio Frame (Opus Chunk with 7-byte header)
      if (
        Buffer.isBuffer(message) ||
        message instanceof Uint8Array ||
        message instanceof ArrayBuffer
      ) {
        const buffer = Buffer.isBuffer(message)
          ? message
          : Buffer.from(message as any);

        pttController.handleBinaryAudioFrame(socket.id, groupUid, userSession, buffer);
        return;
      }

      // 1b. Fallback: Binary Audio Frame received as string with binary header 0x10
      if (typeof message === 'string' && message.charCodeAt(0) === 0x10) {
        const buffer = Buffer.from(message, 'latin1');
        pttController.handleBinaryAudioFrame(socket.id, groupUid, userSession, buffer);
        return;
      }

      // 2. JSON Control Frame (FLOOR_REQUEST, FLOOR_RELEASED)
      let frame: InboundPttControlFrameDto;
      if (typeof message === 'string') {
        frame = JSON.parse(message) as InboundPttControlFrameDto;
      } else if (typeof message === 'object' && message !== null) {
        frame = message as InboundPttControlFrameDto;
      } else {
        return;
      }

      await pttController.handleControlFrame(socket, groupUid, userSession, frame);
    } catch (err) {
      logger.warn({ err }, `[PTT] Malformed control frame received from socket`);
    }
  },

  close: async (ws) => {
    try {
      const socket =
        ((ws.data as unknown as Record<string, unknown>)?.socket as { id: string } | undefined) ||
        (ws as unknown as { id: string });
      const user = (ws.data as unknown as { user?: JwtUserPayload })?.user;
      const groupUid = ws.data?.params?.groupUid || user?.group_uid;
      if (user?.sub && groupUid) {
        await pttController.handleDisconnection(socket.id, groupUid, {
          sub: user.sub,
          name: user.name,
          role: user.role,
          group_uid: user.group_uid,
        });
      }
    } catch (err) {
      logger.error({ err }, '[PTT] Error during WebSocket close lifecycle');
    }
  },
});
