//! Redis-backed session versioning, ported from `session-version.service.ts`.
//!
//! One string key per user, `auth:user:{id}:session_ver`, holds the current version. Access tokens
//! carry the version they were issued under; bumping it revokes every older token at once. The
//! key expires with the refresh token lifetime and is re-created from a valid token if Redis lost
//! it, so a Redis restart does not log everyone out.

use redis::aio::ConnectionManager;
use redis::{AsyncCommands, ErrorKind, RedisError, RedisResult, SetExpiry, SetOptions};

/// Session version store; cloning is cheap.
#[derive(Clone)]
pub struct SessionVersions {
    redis: ConnectionManager,
    ttl_seconds: u64,
}

impl std::fmt::Debug for SessionVersions {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SessionVersions")
            .field("ttl_seconds", &self.ttl_seconds)
            .finish_non_exhaustive()
    }
}

impl SessionVersions {
    /// `ttl_seconds` is the refresh token lifetime (`config.auth.refresh_token_expires_in`).
    pub fn new(redis: ConnectionManager, ttl_seconds: u64) -> Self {
        Self { redis, ttl_seconds }
    }

    pub fn key(user_id: &str) -> String {
        format!("auth:user:{user_id}:session_ver")
    }

    /// Current version, created as 1 when absent; refreshes the TTL when present. Used when issuing
    /// tokens (login, refresh, switch group).
    ///
    /// # Errors
    /// Redis errors, an empty `user_id`, or a stored value that is not an integer.
    pub async fn get_or_init_version(&self, user_id: &str) -> RedisResult<i64> {
        require_user(user_id)?;
        let key = Self::key(user_id);
        let mut redis = self.redis.clone();
        let existing: Option<String> = redis.get(&key).await?;
        if let Some(value) = existing {
            let _: bool = redis.expire(&key, self.ttl()).await?;
            return parse_version(&value);
        }
        let _: () = redis.set_ex(&key, 1, self.ttl_seconds).await?;
        Ok(1)
    }

    /// Atomically increments the version and refreshes the TTL. Revokes outstanding access tokens
    /// on permission changes, removal from a group, deactivation and logout.
    ///
    /// # Errors
    /// Redis errors or an empty `user_id`.
    pub async fn increment_version(&self, user_id: &str) -> RedisResult<i64> {
        require_user(user_id)?;
        let key = Self::key(user_id);
        let mut redis = self.redis.clone();
        let version: i64 = redis.incr(&key, 1).await?;
        let _: bool = redis.expire(&key, self.ttl()).await?;
        Ok(version)
    }

    /// Whether a token issued under `token_version` is still valid.
    ///
    /// Invalid only when the token's version is lower than the stored one. A missing key is
    /// re-created from the token (`SET NX`), which keeps valid tokens working after a Redis flush.
    ///
    /// # Errors
    /// Redis errors.
    pub async fn validate_or_rehydrate(
        &self,
        user_id: &str,
        token_version: i64,
    ) -> RedisResult<bool> {
        if user_id.is_empty() || token_version <= 0 {
            return Ok(false);
        }
        let key = Self::key(user_id);
        let mut redis = self.redis.clone();
        let cached: Option<String> = redis.get(&key).await?;
        match cached {
            None => {
                let options = SetOptions::default()
                    .conditional_set(redis::ExistenceCheck::NX)
                    .with_expiration(SetExpiry::EX(self.ttl_seconds));
                let _: Option<String> = redis.set_options(&key, token_version, options).await?;
                Ok(true)
            }
            // A non-numeric value compares false, like `NaN` in the TypeScript version.
            Some(value) => Ok(value
                .trim()
                .parse::<i64>()
                .is_ok_and(|stored| token_version >= stored)),
        }
    }

    fn ttl(&self) -> i64 {
        i64::try_from(self.ttl_seconds).unwrap_or(i64::MAX)
    }
}

fn require_user(user_id: &str) -> RedisResult<()> {
    if user_id.is_empty() {
        return Err(RedisError::from((
            ErrorKind::Client,
            "user id must not be empty",
        )));
    }
    Ok(())
}

fn parse_version(value: &str) -> RedisResult<i64> {
    value.trim().parse().map_err(|_| {
        RedisError::from((
            ErrorKind::Parse,
            "session version in Redis is not an integer",
        ))
    })
}
