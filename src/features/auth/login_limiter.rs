//! Login throttling in Redis, ported from `login-rate-limiter.service.ts`.
//!
//! Two layers: a per-IP attempt counter (`rl:login:ip:{ip}`) and a per-account lockout. Failed
//! passwords are counted in `rl:login:email:fails:{email}`; reaching the limit sets
//! `rl:login:email:locked:{email}` for the lockout duration and clears the counter.

use redis::AsyncCommands;
use redis::RedisResult;
use redis::aio::ConnectionManager;

use crate::core::config::AuthConfig;

pub struct LoginLimiter<'a> {
    redis: ConnectionManager,
    config: &'a AuthConfig,
}

fn ip_key(ip: &str) -> String {
    format!("rl:login:ip:{}", ip.trim())
}

fn fail_key(email: &str) -> String {
    format!("rl:login:email:fails:{}", email.trim().to_lowercase())
}

fn lock_key(email: &str) -> String {
    format!("rl:login:email:locked:{}", email.trim().to_lowercase())
}

/// Seconds left on `key`, or `fallback` when Redis reports no expiry (-1) or no key (-2).
async fn remaining(redis: &mut ConnectionManager, key: &str, fallback: u64) -> RedisResult<u64> {
    let ttl: i64 = redis.ttl(key).await?;
    Ok(u64::try_from(ttl)
        .ok()
        .filter(|&t| t > 0)
        .unwrap_or(fallback))
}

impl<'a> LoginLimiter<'a> {
    pub fn new(redis: ConnectionManager, config: &'a AuthConfig) -> Self {
        Self { redis, config }
    }

    /// Seconds to wait when `ip` has used up its login attempts.
    ///
    /// # Errors
    /// Redis errors.
    pub async fn ip_blocked_for(&self, ip: &str) -> RedisResult<Option<u64>> {
        let mut redis = self.redis.clone();
        let key = ip_key(ip);
        let count: Option<String> = redis.get(&key).await?;
        // Mirrors `Number.parseInt`: a non-numeric value counts as zero.
        let count = count
            .and_then(|c| crate::core::clean::parse_int_prefix(&c))
            .unwrap_or(0);
        if count < i64::from(self.config.login_rate_limit_ip_max) {
            return Ok(None);
        }
        remaining(&mut redis, &key, self.config.login_rate_limit_duration)
            .await
            .map(Some)
    }

    /// Seconds left on the account lock, if `email` is locked.
    ///
    /// # Errors
    /// Redis errors.
    pub async fn locked_for(&self, email: &str) -> RedisResult<Option<u64>> {
        let mut redis = self.redis.clone();
        let key = lock_key(email);
        let locked: Option<String> = redis.get(&key).await?;
        if locked.is_none_or(|v| v.is_empty()) {
            return Ok(None);
        }
        remaining(&mut redis, &key, self.config.login_lockout_duration)
            .await
            .map(Some)
    }

    /// Counts a failed login against the IP (when known) and the account, locking the account at
    /// the configured limit.
    ///
    /// # Errors
    /// Redis errors.
    pub async fn record_failure(&self, email: &str, ip: Option<&str>) -> RedisResult<()> {
        let mut redis = self.redis.clone();
        if let Some(ip) = ip.filter(|ip| !ip.is_empty()) {
            let key = ip_key(ip);
            let count: i64 = redis.incr(&key, 1).await?;
            if count == 1 {
                let _: bool = redis
                    .expire(&key, secs(self.config.login_rate_limit_duration))
                    .await?;
            }
        }

        let fails_key = fail_key(email);
        let fails: i64 = redis.incr(&fails_key, 1).await?;
        if fails == 1 {
            let _: bool = redis
                .expire(&fails_key, secs(self.config.login_lockout_duration))
                .await?;
        }
        if fails >= i64::from(self.config.login_max_failed_attempts) {
            let _: () = redis
                .set_ex(lock_key(email), 1, self.config.login_lockout_duration)
                .await?;
            let _: () = redis.del(&fails_key).await?;
        }
        Ok(())
    }

    /// Forgets failures and any lock after a successful login.
    ///
    /// # Errors
    /// Redis errors.
    pub async fn reset(&self, email: &str) -> RedisResult<()> {
        let mut redis = self.redis.clone();
        let _: () = redis.del(&[fail_key(email), lock_key(email)]).await?;
        Ok(())
    }
}

fn secs(value: u64) -> i64 {
    i64::try_from(value).unwrap_or(i64::MAX)
}
