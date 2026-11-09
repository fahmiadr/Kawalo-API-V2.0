//! Refresh token sessions in Redis, ported from `auth-redis.datasource.ts`.
//!
//! Key layout (shared with the TypeScript server):
//! * `auth:refresh:{hash}`: JSON [`SessionData`], TTL until expiry
//! * `auth:user:{id}:sessions`, `auth:family:{id}:sessions`: ZSETs of token hashes scored by expiry
//! * `auth:tombstone:{hash}`: family id of a rotated token, for reuse detection
//! * `auth:grace:{hash}`: tokens issued by the rotation, replayed for 15 seconds
//! * `lock:{hash}`: rotation lock

use std::sync::LazyLock;

use redis::aio::ConnectionManager;
use redis::{AsyncCommands, RedisResult, Script, SetExpiry, SetOptions};
use serde::{Deserialize, Serialize};

use crate::core::clean::clean_string;
use crate::features::auth::dto::Tokens;

/// How long the tokens of a rotation can be replayed by a client that retries.
const GRACE_SECONDS: u64 = 15;

/// A refresh token as the server knows it.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RefreshSession {
    pub user_id: String,
    pub token_hash: String,
    pub family_id: String,
    pub active_group_uid: Option<String>,
    /// Unix seconds.
    pub created_at: i64,
    /// Unix seconds.
    pub expires_at: i64,
}

/// The JSON stored under `auth:refresh:{hash}`.
#[derive(Debug, Serialize, Deserialize)]
struct SessionData {
    user_id: String,
    family_id: String,
    active_group_uid: Option<String>,
    created_at: i64,
    expires_at: i64,
}

/// The JSON stored under `auth:grace:{hash}`; camelCase because the TypeScript server wrote it.
#[derive(Debug, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct GraceTokens {
    access_token: String,
    refresh_token: String,
    token_type: String,
    expires_in: u64,
}

impl RefreshSession {
    /// `None` when an essential field is blank or a timestamp is not positive.
    fn to_data(&self) -> Option<SessionData> {
        let user_id = clean_string(Some(&self.user_id))?;
        let family_id = clean_string(Some(&self.family_id))?;
        (self.created_at > 0 && self.expires_at > 0).then(|| SessionData {
            user_id,
            family_id,
            active_group_uid: clean_string(self.active_group_uid.as_deref()),
            created_at: self.created_at,
            expires_at: self.expires_at,
        })
    }
}

fn session_key(hash: &str) -> String {
    format!("auth:refresh:{hash}")
}

fn user_key(user_id: &str) -> String {
    format!("auth:user:{user_id}:sessions")
}

fn family_key(family_id: &str) -> String {
    format!("auth:family:{family_id}:sessions")
}

/// Atomically swaps the old session for the new one; returns the old JSON, or nil when the old
/// session no longer exists (someone else rotated it first).
static ROTATE: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        r"
        local old_key, new_key, tombstone_key = KEYS[1], KEYS[2], KEYS[3]
        local user_key, family_key, grace_key = KEYS[4], KEYS[5], KEYS[6]
        local new_val, ttl, family_id = ARGV[1], tonumber(ARGV[2]), ARGV[3]
        local old_hash, new_hash = ARGV[4], ARGV[5]
        local expires_at, now_unix = tonumber(ARGV[6]), tonumber(ARGV[7])
        local grace_val, grace_ttl = ARGV[8], tonumber(ARGV[9])

        local data = redis.call('GET', old_key)
        if not data then
            return nil
        end

        redis.call('DEL', old_key)
        redis.call('SET', tombstone_key, family_id, 'EX', ttl)
        redis.call('SET', new_key, new_val, 'EX', ttl)
        if grace_val and grace_val ~= '' and grace_ttl and grace_ttl > 0 then
            redis.call('SET', grace_key, grace_val, 'EX', grace_ttl)
        end

        redis.call('ZREMRANGEBYSCORE', user_key, '-inf', now_unix)
        redis.call('ZREM', user_key, old_hash)
        redis.call('ZADD', user_key, expires_at, new_hash)
        redis.call('EXPIRE', user_key, ttl)

        redis.call('ZREMRANGEBYSCORE', family_key, '-inf', now_unix)
        redis.call('ZREM', family_key, old_hash)
        redis.call('ZADD', family_key, expires_at, new_hash)
        redis.call('EXPIRE', family_key, ttl)

        return data
        ",
    )
});

static RELEASE_LOCK: LazyLock<Script> = LazyLock::new(|| {
    Script::new(
        r#"
        if redis.call("get", KEYS[1]) == ARGV[1] then
            return redis.call("del", KEYS[1])
        else
            return 0
        end
        "#,
    )
});

pub struct Sessions {
    redis: ConnectionManager,
}

impl Sessions {
    pub fn new(redis: ConnectionManager) -> Self {
        Self { redis }
    }

    /// Stores a session and indexes it under its user and family. Blank or invalid sessions are
    /// silently dropped, like the TypeScript datasource.
    ///
    /// # Errors
    /// Redis errors.
    pub async fn save(&self, session: &RefreshSession) -> RedisResult<()> {
        let Some(data) = session.to_data() else {
            return Ok(());
        };
        let ttl = ttl_seconds(session.expires_at);
        let now = now_unix();
        let key = session_key(&session.token_hash);
        let (user_key, family_key) = (user_key(&data.user_id), family_key(&data.family_id));
        let json = serde_json::to_string(&data).map_err(json_error)?;

        let mut pipe = redis::pipe();
        pipe.set_ex(&key, json, ttl.unsigned_abs()).ignore();
        for index in [&user_key, &family_key] {
            pipe.zrembyscore(index, "-inf", now)
                .ignore()
                .zadd(index, &session.token_hash, session.expires_at)
                .ignore()
                .expire(index, ttl)
                .ignore();
        }
        let mut redis = self.redis.clone();
        pipe.query_async::<()>(&mut redis).await
    }

    /// The live session for a token hash; corrupt entries read as absent.
    ///
    /// # Errors
    /// Redis errors.
    pub async fn get(&self, token_hash: &str) -> RedisResult<Option<RefreshSession>> {
        let mut redis = self.redis.clone();
        let raw: Option<String> = redis.get(session_key(token_hash)).await?;
        let Some(data) = raw.and_then(|raw| serde_json::from_str::<SessionData>(&raw).ok()) else {
            return Ok(None);
        };
        let (Some(user_id), Some(family_id)) = (
            clean_string(Some(&data.user_id)),
            clean_string(Some(&data.family_id)),
        ) else {
            return Ok(None);
        };
        if data.created_at <= 0 || data.expires_at <= 0 {
            return Ok(None);
        }
        Ok(Some(RefreshSession {
            user_id,
            token_hash: token_hash.to_owned(),
            family_id,
            active_group_uid: clean_string(data.active_group_uid.as_deref()),
            created_at: data.created_at,
            expires_at: data.expires_at,
        }))
    }

    /// Family of a token that was already rotated, if the tombstone is still around.
    ///
    /// # Errors
    /// Redis errors.
    pub async fn rotated_family(&self, token_hash: &str) -> RedisResult<Option<String>> {
        let mut redis = self.redis.clone();
        let family: Option<String> = redis.get(format!("auth:tombstone:{token_hash}")).await?;
        Ok(clean_string(family.as_deref()))
    }

    /// Tokens issued when `token_hash` was rotated, while the grace window is open.
    ///
    /// # Errors
    /// Redis errors.
    pub async fn grace_tokens(&self, token_hash: &str) -> RedisResult<Option<Tokens>> {
        let mut redis = self.redis.clone();
        let raw: Option<String> = redis.get(format!("auth:grace:{token_hash}")).await?;
        Ok(raw
            .and_then(|raw| serde_json::from_str::<GraceTokens>(&raw).ok())
            .map(|g| Tokens {
                access_token: g.access_token,
                refresh_token: g.refresh_token,
                token_type: g.token_type,
                expires_in: g.expires_in,
            }))
    }

    /// Replaces `old_hash` with `new` and records the grace tokens, all in one Lua script.
    /// `false` when the old session was already gone.
    ///
    /// # Errors
    /// Redis errors.
    pub async fn rotate(
        &self,
        old_hash: &str,
        new: &RefreshSession,
        issued: &Tokens,
    ) -> RedisResult<bool> {
        let Some(data) = new.to_data() else {
            return Ok(false);
        };
        let grace = serde_json::to_string(&GraceTokens {
            access_token: issued.access_token.clone(),
            refresh_token: issued.refresh_token.clone(),
            token_type: issued.token_type.clone(),
            expires_in: issued.expires_in,
        })
        .map_err(json_error)?;
        let mut redis = self.redis.clone();
        let old: Option<String> = ROTATE
            .key(session_key(old_hash))
            .key(session_key(&new.token_hash))
            .key(format!("auth:tombstone:{old_hash}"))
            .key(user_key(&data.user_id))
            .key(family_key(&data.family_id))
            .key(format!("auth:grace:{old_hash}"))
            .arg(serde_json::to_string(&data).map_err(json_error)?)
            .arg(ttl_seconds(new.expires_at))
            .arg(&data.family_id)
            .arg(old_hash)
            .arg(&new.token_hash)
            .arg(new.expires_at)
            .arg(now_unix())
            .arg(grace)
            .arg(GRACE_SECONDS)
            .invoke_async(&mut redis)
            .await?;
        Ok(old.is_some())
    }

    /// Deletes every live token of a family (token reuse was detected).
    ///
    /// # Errors
    /// Redis errors.
    pub async fn revoke_family(&self, family_id: &str) -> RedisResult<()> {
        let Some(family_id) = clean_string(Some(family_id)) else {
            return Ok(());
        };
        let index = family_key(&family_id);
        let mut redis = self.redis.clone();
        let hashes: Vec<String> = redis.zrange(&index, 0, -1).await?;
        if hashes.is_empty() {
            return Ok(());
        }
        let mut pipe = redis::pipe();
        for hash in &hashes {
            pipe.del(session_key(hash)).ignore();
        }
        pipe.del(&index).ignore();
        pipe.query_async::<()>(&mut redis).await
    }

    /// Deletes one session, and drops it from the user's index when `user_id` is known.
    ///
    /// # Errors
    /// Redis errors.
    pub async fn delete(&self, token_hash: &str, user_id: Option<&str>) -> RedisResult<()> {
        let mut redis = self.redis.clone();
        let mut pipe = redis::pipe();
        pipe.del(session_key(token_hash)).ignore();
        if let Some(user_id) = user_id.and_then(|id| clean_string(Some(id))) {
            pipe.zrem(user_key(&user_id), token_hash).ignore();
        }
        pipe.query_async::<()>(&mut redis).await
    }

    /// `SET lock:{key} {value} NX EX {ttl}`.
    ///
    /// # Errors
    /// Redis errors.
    pub async fn acquire_lock(
        &self,
        key: &str,
        value: &str,
        ttl_seconds: u64,
    ) -> RedisResult<bool> {
        let mut redis = self.redis.clone();
        let options = SetOptions::default()
            .conditional_set(redis::ExistenceCheck::NX)
            .with_expiration(SetExpiry::EX(ttl_seconds));
        let reply: Option<String> = redis
            .set_options(format!("lock:{key}"), value, options)
            .await?;
        Ok(reply.is_some())
    }

    /// Releases the lock only if `value` is still the holder.
    ///
    /// # Errors
    /// Redis errors.
    pub async fn release_lock(&self, key: &str, value: &str) -> RedisResult<()> {
        let mut redis = self.redis.clone();
        let _: i64 = RELEASE_LOCK
            .key(format!("lock:{key}"))
            .arg(value)
            .invoke_async(&mut redis)
            .await?;
        Ok(())
    }
}

/// Remaining lifetime in seconds, at least one because `SET EX 0` is an error.
fn ttl_seconds(expires_at: i64) -> i64 {
    (expires_at - now_unix()).max(1)
}

pub fn now_unix() -> i64 {
    chrono::Utc::now().timestamp()
}

fn json_error(err: serde_json::Error) -> redis::RedisError {
    redis::RedisError::from((
        redis::ErrorKind::Client,
        "could not serialise session",
        err.to_string(),
    ))
}
