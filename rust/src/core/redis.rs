//! Redis connection, ported from `redis.service.ts`.

use std::time::{Duration, Instant};

use redis::aio::{ConnectionManager, ConnectionManagerConfig};
use redis::{Client, RedisResult};

use crate::core::config::Config;
use crate::core::db::HealthStatus;

const CONNECT_TIMEOUT: Duration = Duration::from_millis(5000);
const COMMAND_TIMEOUT: Duration = Duration::from_millis(3000);
const COMMAND_RETRIES: usize = 3;
const STARTUP_RETRIES: u32 = 5;
const STARTUP_RETRY_DELAY: Duration = Duration::from_secs(2);

/// Redis client and its auto-reconnecting multiplexed connection.
///
/// `client` is kept for dedicated connections such as pub/sub subscribers; everything else should
/// clone `manager`.
#[derive(Clone)]
pub struct Redis {
    pub client: Client,
    pub manager: ConnectionManager,
}

impl std::fmt::Debug for Redis {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Redis").finish_non_exhaustive()
    }
}

/// Connects and PINGs with the same startup retry as the TypeScript `RedisService.ping`.
///
/// # Errors
/// The last connection error once all attempts are used.
pub async fn connect(config: &Config) -> RedisResult<Redis> {
    connect_with_retry(config, STARTUP_RETRIES, STARTUP_RETRY_DELAY).await
}

pub(crate) async fn connect_with_retry(
    config: &Config,
    retries: u32,
    delay: Duration,
) -> RedisResult<Redis> {
    let client = Client::open(config.redis_url.as_str())?;
    let manager_config = ConnectionManagerConfig::new()
        .set_connection_timeout(Some(CONNECT_TIMEOUT))
        .set_response_timeout(Some(COMMAND_TIMEOUT))
        .set_number_of_retries(COMMAND_RETRIES);
    let mut attempt = 1;
    loop {
        let started = Instant::now();
        let result = async {
            let mut manager =
                ConnectionManager::new_with_config(client.clone(), manager_config.clone()).await?;
            let pong: String = redis::cmd("PING").query_async(&mut manager).await?;
            Ok::<_, redis::RedisError>((manager, pong))
        }
        .await;
        match result {
            Ok((manager, pong)) => {
                tracing::info!(
                    "[Redis] connected and ready for cache/session ({}ms, {pong})",
                    started.elapsed().as_millis()
                );
                return Ok(Redis { client, manager });
            }
            Err(err) if attempt >= retries => {
                tracing::error!("[Redis] could not connect after {retries} attempts: {err}");
                return Err(err);
            }
            Err(err) => {
                tracing::warn!(
                    "[Redis] connection attempt {attempt}/{retries} failed ({err}); retrying in {}s",
                    delay.as_secs_f32()
                );
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

/// PING for `/health/ready`.
pub async fn health_check(manager: &ConnectionManager) -> HealthStatus {
    let started = Instant::now();
    let mut conn = manager.clone();
    match redis::cmd("PING").query_async::<String>(&mut conn).await {
        Ok(_) => HealthStatus::healthy(started.elapsed()),
        Err(err) => HealthStatus::unhealthy(err),
    }
}
