//! Shared application state.

use std::sync::Arc;

use redis::aio::ConnectionManager;
use sqlx::MySqlPool;

use crate::core::config::Config;
use crate::core::redis::Redis;
use crate::core::security::{RsaKeyManager, SessionVersions};

/// Everything handlers need; every field is a cheap handle, so cloning is free.
///
/// ```ignore
/// async fn list(State(state): State<AppState>) {
///     let rows = sqlx::query("SELECT 1").fetch_all(&state.db).await;
/// }
/// ```
#[derive(Clone, Debug)]
pub struct AppState {
    pub config: Arc<Config>,
    pub db: MySqlPool,
    /// Shared command connection; clone it per command (`let mut redis = state.redis.clone()`).
    pub redis: ConnectionManager,
    /// For dedicated connections (pub/sub subscribers): `state.redis_client.get_async_pubsub()`.
    pub redis_client: redis::Client,
    pub keys: RsaKeyManager,
    pub sessions: SessionVersions,
}

impl AppState {
    pub fn new(config: Arc<Config>, db: MySqlPool, redis: Redis, keys: RsaKeyManager) -> Self {
        let sessions =
            SessionVersions::new(redis.manager.clone(), config.auth.refresh_token_expires_in);
        Self {
            config,
            db,
            redis: redis.manager,
            redis_client: redis.client,
            keys,
            sessions,
        }
    }
}
