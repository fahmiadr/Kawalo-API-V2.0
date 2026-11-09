//! MariaDB connection pool, ported from `prisma.service.ts`.

use std::str::FromStr;
use std::time::{Duration, Instant};

use serde::Serialize;
use sqlx::mysql::{MySqlConnectOptions, MySqlPoolOptions};
use sqlx::{ConnectOptions, MySqlPool};

use crate::core::config::Config;

const DEFAULT_CONNECT_TIMEOUT: Duration = Duration::from_millis(5000);
const DEFAULT_POOL_SIZE: u32 = 10;
const STARTUP_RETRIES: u32 = 5;
const STARTUP_RETRY_DELAY: Duration = Duration::from_secs(2);
const HEALTH_TIMEOUT: Duration = Duration::from_millis(3000);
/// Queries slower than this are logged at WARN.
const SLOW_QUERY: Duration = Duration::from_millis(100);

/// Result of a dependency probe, serialised as in `/health/ready`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct HealthStatus {
    pub status: Health,
    #[serde(rename = "latencyMs", skip_serializing_if = "Option::is_none")]
    pub latency_ms: Option<u64>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub error: Option<String>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum Health {
    Healthy,
    Unhealthy,
}

impl HealthStatus {
    pub fn healthy(elapsed: Duration) -> Self {
        Self {
            status: Health::Healthy,
            latency_ms: Some(u64::try_from(elapsed.as_millis()).unwrap_or(u64::MAX)),
            error: None,
        }
    }

    pub fn unhealthy(error: impl ToString) -> Self {
        Self {
            status: Health::Unhealthy,
            latency_ms: None,
            error: Some(error.to_string()),
        }
    }

    pub fn is_healthy(&self) -> bool {
        self.status == Health::Healthy
    }
}

/// Pool options derived from `DATABASE_URL`.
///
/// Like the TypeScript normaliser this accepts `mysql://` and `mariadb://`, the Prisma-style
/// `connection_limit` / `connect_timeout` (milliseconds, driver semantics) and their camelCase
/// forms. `socketTimeout` has no sqlx equivalent and is ignored.
pub fn pool_options(
    database_url: &str,
) -> Result<(MySqlConnectOptions, MySqlPoolOptions), sqlx::Error> {
    let url = database_url.replacen("mariadb:", "mysql:", 1);
    let query = url.split_once('?').map(|(_, q)| q).unwrap_or_default();
    let param = |names: &[&str]| {
        query
            .split('&')
            .filter_map(|kv| kv.split_once('='))
            .find(|(k, _)| names.contains(k))
            .and_then(|(_, v)| v.parse::<u64>().ok())
    };
    let max_connections = param(&["connection_limit", "connectionLimit"])
        .and_then(|n| u32::try_from(n).ok())
        .filter(|&n| n > 0)
        .unwrap_or(DEFAULT_POOL_SIZE);
    let connect_timeout = param(&["connect_timeout", "connectTimeout"])
        .map_or(DEFAULT_CONNECT_TIMEOUT, Duration::from_millis);

    let options = MySqlConnectOptions::from_str(&url)?
        .log_slow_statements(log::LevelFilter::Warn, SLOW_QUERY);
    let pool = MySqlPoolOptions::new()
        .max_connections(max_connections)
        .acquire_timeout(connect_timeout);
    Ok((options, pool))
}

/// Connects the pool and verifies it with `SELECT 1`, retrying like `PrismaService.connect`.
///
/// # Errors
/// The last connection error once all attempts are used.
pub async fn connect(config: &Config) -> Result<MySqlPool, sqlx::Error> {
    connect_with_retry(config, STARTUP_RETRIES, STARTUP_RETRY_DELAY).await
}

pub(crate) async fn connect_with_retry(
    config: &Config,
    retries: u32,
    delay: Duration,
) -> Result<MySqlPool, sqlx::Error> {
    let (options, pool_options) = pool_options(&config.database_url)?;
    let mut attempt = 1;
    loop {
        let started = Instant::now();
        let result = async {
            let pool = pool_options.clone().connect_with(options.clone()).await?;
            sqlx::query("SELECT 1").execute(&pool).await?;
            Ok::<_, sqlx::Error>(pool)
        }
        .await;
        match result {
            Ok(pool) => {
                tracing::info!(
                    "[DB:Pool] MariaDB connection pool is up ({}ms)",
                    started.elapsed().as_millis()
                );
                return Ok(pool);
            }
            Err(err) if attempt >= retries => {
                tracing::error!(
                    "[DB:Pool] could not connect to MariaDB after {retries} attempts: {err}"
                );
                return Err(err);
            }
            Err(err) => {
                tracing::warn!(
                    "[DB:Pool] connection attempt {attempt}/{retries} failed ({err}); retrying in {}s",
                    delay.as_secs_f32()
                );
                tokio::time::sleep(delay).await;
                attempt += 1;
            }
        }
    }
}

/// `SELECT 1` bounded by a 3 s timeout, for `/health/ready`.
pub async fn health_check(pool: &MySqlPool) -> HealthStatus {
    let started = Instant::now();
    match tokio::time::timeout(HEALTH_TIMEOUT, sqlx::query("SELECT 1").execute(pool)).await {
        Ok(Ok(_)) => HealthStatus::healthy(started.elapsed()),
        Ok(Err(err)) => HealthStatus::unhealthy(err),
        Err(_) => HealthStatus::unhealthy(format!(
            "Database health check timed out (> {}ms)",
            HEALTH_TIMEOUT.as_millis()
        )),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn accepts_mysql_and_mariadb_schemes() {
        assert!(pool_options("mysql://root:@localhost:3306/kawalo").is_ok());
        assert!(pool_options("mariadb://root:@localhost:3306/kawalo").is_ok());
    }

    #[test]
    fn pool_size_and_timeout_follow_the_url_parameters() {
        let (_, pool) = pool_options("mysql://root:@h/db?connection_limit=3&connect_timeout=1500")
            .expect("options");
        assert_eq!(pool.get_max_connections(), 3);
        assert_eq!(pool.get_acquire_timeout(), Duration::from_millis(1500));
        let (_, pool) = pool_options("mysql://root:@h/db?connectionLimit=7").expect("options");
        assert_eq!(pool.get_max_connections(), 7);
        let (_, pool) = pool_options("mysql://root:@h/db").expect("options");
        assert_eq!(
            (pool.get_max_connections(), pool.get_acquire_timeout()),
            (10, DEFAULT_CONNECT_TIMEOUT)
        );
    }

    #[test]
    fn health_status_json_shape() {
        let up =
            serde_json::to_value(HealthStatus::healthy(Duration::from_millis(7))).expect("json");
        assert_eq!(up, serde_json::json!({"status": "healthy", "latencyMs": 7}));
        let down = serde_json::to_value(HealthStatus::unhealthy("boom")).expect("json");
        assert_eq!(
            down,
            serde_json::json!({"status": "unhealthy", "error": "boom"})
        );
    }
}
