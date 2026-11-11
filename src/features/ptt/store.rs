//! Redis side of PTT: the cluster-wide floor lock and presence set.
//!
//! The Lua scripts are the TypeScript ones verbatim, so Rust and TypeScript instances can share a
//! Redis during a rolling migration.

use std::time::{SystemTime, UNIX_EPOCH};

use redis::aio::ConnectionManager;
use redis::{AsyncCommands, RedisError, Script};
use serde::{Deserialize, Serialize};

use super::frames::Speaker;

/// Presence sets expire a day after the last join.
const MEMBERS_TTL_SECONDS: i64 = 86_400;

const ACQUIRE_LUA: &str = r"
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
";

const EXTEND_LUA: &str = r"
local current = redis.call('GET', KEYS[1])
if current then
  local data = cjson.decode(current)
  if data.userId == ARGV[1] then
    redis.call('EXPIRE', KEYS[1], ARGV[2])
    return 1
  end
end
return 0
";

const RELEASE_LUA: &str = r"
local current = redis.call('GET', KEYS[1])
if current then
  local data = cjson.decode(current)
  if data.userId == ARGV[1] then
    redis.call('DEL', KEYS[1])
    return 1
  end
end
return 0
";

#[derive(Debug, thiserror::Error)]
pub enum StoreError {
    #[error("redis: {0}")]
    Redis(#[from] RedisError),
    #[error("floor script returned invalid JSON: {0}")]
    Json(#[from] serde_json::Error),
}

/// The value stored under the floor key.
#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
pub struct FloorLock<'a> {
    pub user_id: &'a str,
    pub name: &'a str,
    pub role: &'a str,
    pub socket_id: &'a str,
    granted_at_ms: u64,
    burst_start_ms: u64,
}

impl<'a> FloorLock<'a> {
    pub fn new(user_id: &'a str, name: &'a str, role: &'a str, socket_id: &'a str) -> Self {
        let now_ms = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .map_or(0, |d| u64::try_from(d.as_millis()).unwrap_or(u64::MAX));
        Self {
            user_id,
            name,
            role,
            socket_id,
            granted_at_ms: now_ms,
            burst_start_ms: now_ms,
        }
    }
}

#[derive(Debug, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Acquired {
    pub granted: bool,
    pub preempted: bool,
    pub previous_speaker: Option<Speaker>,
    pub current_speaker: Option<Speaker>,
}

#[derive(Debug)]
pub struct FloorStore {
    redis: ConnectionManager,
    acquire: Script,
    extend: Script,
    release: Script,
}

impl FloorStore {
    pub fn new(redis: ConnectionManager) -> Self {
        Self {
            redis,
            acquire: Script::new(ACQUIRE_LUA),
            extend: Script::new(EXTEND_LUA),
            release: Script::new(RELEASE_LUA),
        }
    }

    fn floor_key(group: &str) -> String {
        format!("ptt:floor:{group}")
    }

    fn members_key(group: &str) -> String {
        format!("ptt:room:{group}:members")
    }

    fn member_info_key(group: &str) -> String {
        format!("ptt:room:{group}:member_info")
    }

    /// Atomically takes the floor if it is free, or from a non-admin speaker when the requester
    /// is an admin.
    pub async fn acquire_floor(
        &self,
        group: &str,
        lock: &FloorLock<'_>,
        lease_seconds: u32,
    ) -> Result<Acquired, StoreError> {
        let payload = serde_json::to_string(lock)?;
        let mut conn = self.redis.clone();
        let raw: String = self
            .acquire
            .key(Self::floor_key(group))
            .arg(payload)
            .arg(lock.role)
            .arg(lease_seconds)
            .invoke_async(&mut conn)
            .await?;
        Ok(serde_json::from_str(&raw)?)
    }

    /// Renews the lease, but only while `user_id` still holds the floor.
    pub async fn extend_lease(
        &self,
        group: &str,
        user_id: &str,
        lease_seconds: u32,
    ) -> Result<bool, StoreError> {
        let mut conn = self.redis.clone();
        let renewed: i64 = self
            .extend
            .key(Self::floor_key(group))
            .arg(user_id)
            .arg(lease_seconds)
            .invoke_async(&mut conn)
            .await?;
        Ok(renewed == 1)
    }

    /// Deletes the floor, but only if `user_id` holds it.
    pub async fn release_floor(&self, group: &str, user_id: &str) -> Result<bool, StoreError> {
        let mut conn = self.redis.clone();
        let released: i64 = self
            .release
            .key(Self::floor_key(group))
            .arg(user_id)
            .invoke_async(&mut conn)
            .await?;
        Ok(released == 1)
    }

    /// The current speaker; an unparsable value counts as an idle floor.
    pub async fn current_floor(&self, group: &str) -> Result<Option<Speaker>, StoreError> {
        let mut conn = self.redis.clone();
        let raw: Option<String> = conn.get(Self::floor_key(group)).await?;
        Ok(raw.and_then(|raw| serde_json::from_str(&raw).ok()))
    }

    /// Adds `member` to the presence set (and its identity to the info hash behind the REST member
    /// list) and returns the new size, in one round trip. Keeping it to one round trip matters:
    /// the join broadcast must not slip behind a neighbour's departure broadcast.
    pub async fn add_member(&self, group: &str, member: &Speaker) -> Result<u64, StoreError> {
        let key = Self::members_key(group);
        let info_key = Self::member_info_key(group);
        let mut conn = self.redis.clone();
        let (count,): (u64,) = redis::pipe()
            .sadd(&key, &member.user_id)
            .ignore()
            .expire(&key, MEMBERS_TTL_SECONDS)
            .ignore()
            .hset(&info_key, &member.user_id, serde_json::to_string(member)?)
            .ignore()
            .expire(&info_key, MEMBERS_TTL_SECONDS)
            .ignore()
            .scard(&key)
            .query_async(&mut conn)
            .await?;
        Ok(count)
    }

    pub async fn remove_member(&self, group: &str, user_id: &str) -> Result<u64, StoreError> {
        let key = Self::members_key(group);
        let mut conn = self.redis.clone();
        let (count,): (u64,) = redis::pipe()
            .srem(&key, user_id)
            .ignore()
            .hdel(Self::member_info_key(group), user_id)
            .ignore()
            .scard(&key)
            .query_async(&mut conn)
            .await?;
        Ok(count)
    }

    /// Connected members by name (case-insensitive), then id. Ids without stored info (joined
    /// through an instance that predates `member_info`) are skipped.
    pub async fn members(&self, group: &str) -> Result<Vec<Speaker>, StoreError> {
        let mut conn = self.redis.clone();
        let ids: Vec<String> = conn.smembers(Self::members_key(group)).await?;
        if ids.is_empty() {
            return Ok(Vec::new());
        }
        let raw: Vec<Option<String>> = redis::cmd("HMGET")
            .arg(Self::member_info_key(group))
            .arg(&ids)
            .query_async(&mut conn)
            .await?;
        let mut members: Vec<Speaker> = raw
            .into_iter()
            .flatten()
            .filter_map(|raw| serde_json::from_str(&raw).ok())
            .collect();
        members.sort_by_cached_key(|m| (m.name.to_lowercase(), m.user_id.clone()));
        Ok(members)
    }

    pub async fn member_count(&self, group: &str) -> Result<u64, StoreError> {
        let mut conn = self.redis.clone();
        Ok(conn.scard(Self::members_key(group)).await?)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    async fn store() -> FloorStore {
        let url = std::env::var("REDIS_URL").expect("REDIS_URL (see scripts/dev-services.sh)");
        let client = redis::Client::open(url).expect("redis url");
        FloorStore::new(ConnectionManager::new(client).await.expect("redis"))
    }

    fn group() -> String {
        format!("store-{}", uuid::Uuid::new_v4())
    }

    async fn ttl_ms(store: &FloorStore, group: &str) -> i64 {
        let mut conn = store.redis.clone();
        redis::cmd("PTTL")
            .arg(FloorStore::floor_key(group))
            .query_async(&mut conn)
            .await
            .expect("pttl")
    }

    async fn acquire(store: &FloorStore, group: &str, user: &str, role: &str) -> Acquired {
        let lock = FloorLock::new(user, user, role, "sock");
        store.acquire_floor(group, &lock, 5).await.expect("acquire")
    }

    #[tokio::test]
    async fn idle_floor_is_granted_with_a_lease() {
        let (store, g) = (store().await, group());
        let result = acquire(&store, &g, "u1", "GUARD").await;
        assert!(result.granted && !result.preempted);
        assert!((1..=5000).contains(&ttl_ms(&store, &g).await));
        let current = store.current_floor(&g).await.expect("get").expect("held");
        assert_eq!(
            (current.user_id.as_str(), current.role.as_str()),
            ("u1", "GUARD")
        );
    }

    #[tokio::test]
    async fn arbitration_is_two_tier() {
        let (store, g) = (store().await, group());
        acquire(&store, &g, "guard", "GUARD").await;

        let other_guard = acquire(&store, &g, "guard2", "GUARD").await;
        assert!(!other_guard.granted);
        assert_eq!(
            other_guard.current_speaker.expect("speaker").user_id,
            "guard"
        );

        let admin = acquire(&store, &g, "admin", "ADMIN").await;
        assert!(admin.granted && admin.preempted);
        assert_eq!(admin.previous_speaker.expect("previous").user_id, "guard");

        for (user, role) in [("guard", "GUARD"), ("admin2", "ADMIN")] {
            let denied = acquire(&store, &g, user, role).await;
            assert!(
                !denied.granted,
                "{role} must not take the floor from an admin"
            );
            assert_eq!(denied.current_speaker.expect("speaker").user_id, "admin");
        }
    }

    #[tokio::test]
    async fn lease_is_extended_and_released_only_by_the_holder() {
        let (store, g) = (store().await, group());
        acquire(&store, &g, "u1", "GUARD").await;

        assert!(!store.extend_lease(&g, "u2", 5).await.expect("extend"));
        assert!(!store.release_floor(&g, "u2").await.expect("release"));
        assert!(store.current_floor(&g).await.expect("get").is_some());

        let mut conn = store.redis.clone();
        let _: () = redis::cmd("PEXPIRE")
            .arg(FloorStore::floor_key(&g))
            .arg(100)
            .query_async(&mut conn)
            .await
            .expect("shorten");
        assert!(store.extend_lease(&g, "u1", 5).await.expect("extend"));
        assert!(ttl_ms(&store, &g).await > 1000);

        assert!(store.release_floor(&g, "u1").await.expect("release"));
        assert!(store.current_floor(&g).await.expect("get").is_none());
    }

    #[tokio::test]
    async fn presence_is_a_set_of_user_ids_with_a_day_of_retention() {
        let (store, g) = (store().await, group());
        let member = |id: &str| Speaker {
            user_id: id.to_owned(),
            name: id.to_owned(),
            role: "GUARD".to_owned(),
        };
        assert_eq!(store.add_member(&g, &member("u1")).await.expect("add"), 1);
        assert_eq!(store.add_member(&g, &member("u1")).await.expect("add"), 1);
        assert_eq!(store.add_member(&g, &member("u2")).await.expect("add"), 2);
        assert_eq!(store.member_count(&g).await.expect("count"), 2);

        let mut conn = store.redis.clone();
        let ttl: i64 = redis::cmd("TTL")
            .arg(FloorStore::members_key(&g))
            .query_async(&mut conn)
            .await
            .expect("ttl");
        assert!((86_000..=86_400).contains(&ttl), "ttl {ttl}");

        assert_eq!(store.remove_member(&g, "u1").await.expect("remove"), 1);
        assert_eq!(store.remove_member(&g, "u1").await.expect("remove"), 1);
    }

    #[tokio::test]
    async fn unparsable_floor_value_counts_as_idle() {
        let (store, g) = (store().await, group());
        let mut conn = store.redis.clone();
        let _: () = redis::cmd("SET")
            .arg(FloorStore::floor_key(&g))
            .arg("not json")
            .query_async(&mut conn)
            .await
            .expect("set");
        assert!(store.current_floor(&g).await.expect("get").is_none());
    }
}
