import { redis } from '@core/database/redis.service';
import type { PttFloorSpeaker } from '@features/ptt/domain/ptt-floor.entity';
import type Redis from 'ioredis';

export interface PttAcquireResult {
  granted: boolean;
  preempted: boolean;
  previousSpeaker?: PttFloorSpeaker;
  currentSpeaker?: PttFloorSpeaker;
}

export class PttRedisRepository {
  private readonly client: Redis;

  constructor(customRedis?: Redis) {
    this.client = customRedis ?? redis;
  }

  public getFloorKey(groupUid: string): string {
    return `ptt:floor:${groupUid}`;
  }

  public getMembersKey(groupUid: string): string {
    return `ptt:room:${groupUid}:members`;
  }

  /**
   * Atomic Floor Acquisition via Redis Lua script.
   * Handles 2-tier preemption (Admin overrides Guard) and Server FIFO atomically.
   */
  public async acquireFloor(
    groupUid: string,
    speaker: PttFloorSpeaker,
    leaseTtlSeconds = 2,
  ): Promise<PttAcquireResult> {
    const key = this.getFloorKey(groupUid);
    const speakerPayload = JSON.stringify({
      userId: speaker.userId,
      name: speaker.name,
      role: speaker.role,
      socketId: speaker.socketId,
      grantedAtMs: Date.now(),
      burstStartMs: Date.now(),
    });

    const luaScript = `
      local current = redis.call('GET', KEYS[1])
      if not current then
        redis.call('SET', KEYS[1], ARGV[1], 'EX', ARGV[3])
        return cjson.encode({ granted = true, preempted = false })
      end

      local currentData = cjson.decode(current)
      local currentRole = string.upper(currentData.role or 'GUARD')
      local reqRole = string.upper(ARGV[2])

      if reqRole == 'ADMIN' and currentRole ~= 'ADMIN' then
        redis.call('SET', KEYS[1], ARGV[1], 'EX', ARGV[3])
        return cjson.encode({ granted = true, preempted = true, previousSpeaker = currentData })
      end

      return cjson.encode({ granted = false, preempted = false, currentSpeaker = currentData })
    `;

    const rawResult = (await this.client.eval(
      luaScript,
      1,
      key,
      speakerPayload,
      speaker.role,
      leaseTtlSeconds,
    )) as string;

    const parsed = JSON.parse(rawResult) as {
      granted: boolean;
      preempted: boolean;
      previousSpeaker?: PttFloorSpeaker;
      currentSpeaker?: PttFloorSpeaker;
    };

    return parsed;
  }

  /**
   * Atomic Floor Lease Extension (1Hz Debounced).
   * Renews lease only if the current floor holder matches userId.
   */
  public async extendFloorLease(
    groupUid: string,
    userId: string,
    leaseTtlSeconds = 2,
  ): Promise<boolean> {
    const key = this.getFloorKey(groupUid);

    const luaScript = `
      local current = redis.call('GET', KEYS[1])
      if current then
        local data = cjson.decode(current)
        if data.userId == ARGV[1] then
          redis.call('EXPIRE', KEYS[1], ARGV[2])
          return 1
        end
      end
      return 0
    `;

    const res = await this.client.eval(luaScript, 1, key, userId, leaseTtlSeconds);
    return res === 1;
  }

  /**
   * Atomic Floor Release.
   * Deletes floor lock only if the releasing user matches the current speaker.
   */
  public async releaseFloor(groupUid: string, userId: string): Promise<boolean> {
    const key = this.getFloorKey(groupUid);

    const luaScript = `
      local current = redis.call('GET', KEYS[1])
      if current then
        local data = cjson.decode(current)
        if data.userId == ARGV[1] then
          redis.call('DEL', KEYS[1])
          return 1
        end
      end
      return 0
    `;

    const res = await this.client.eval(luaScript, 1, key, userId);
    return res === 1;
  }

  /**
   * Get current floor speaker from Redis or null if idle.
   */
  public async getCurrentFloor(groupUid: string): Promise<PttFloorSpeaker | null> {
    const key = this.getFloorKey(groupUid);
    const data = await this.client.get(key);
    if (!data) {
      return null;
    }
    try {
      const parsed = JSON.parse(data) as PttFloorSpeaker;
      return parsed;
    } catch {
      return null;
    }
  }

  /**
   * Force reset channel floor in Redis (e.g. after TOT expiration or room cleanup).
   */
  public async forceResetFloor(groupUid: string): Promise<void> {
    const key = this.getFloorKey(groupUid);
    await this.client.del(key);
  }

  /* ---------------- Presence Tracking via Redis Sets ---------------- */

  public async addMember(groupUid: string, userId: string): Promise<number> {
    const key = this.getMembersKey(groupUid);
    await this.client.sadd(key, userId);
    await this.client.expire(key, 86400); // 24-hour retention
    return this.client.scard(key);
  }

  public async removeMember(groupUid: string, userId: string): Promise<number> {
    const key = this.getMembersKey(groupUid);
    await this.client.srem(key, userId);
    return this.client.scard(key);
  }

  public async getMemberCount(groupUid: string): Promise<number> {
    const key = this.getMembersKey(groupUid);
    return this.client.scard(key);
  }

  public async getMembers(groupUid: string): Promise<string[]> {
    const key = this.getMembersKey(groupUid);
    return this.client.smembers(key);
  }
}

export const pttRedisRepository = new PttRedisRepository();
