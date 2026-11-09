//! Fixed-window rate limit per client IP, equivalent to `elysia-rate-limit` as configured in
//! `app.ts`: `RATE_LIMIT_MAX` requests per `RATE_LIMIT_DURATION` ms, counted on every route.

use std::collections::HashMap;
use std::sync::{Arc, Mutex};
use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::{Request, State};
use axum::http::{HeaderName, HeaderValue};
use axum::middleware::Next;
use axum::response::{IntoResponse, Response};

use crate::core::client_ip::client_ip_from;
use crate::core::error::ApiError;
use crate::core::state::AppState;

/// Upper bound on tracked IPs, as in the TypeScript store (`AllocQuickLRU`, 5000).
const MAX_TRACKED_CLIENTS: usize = 5000;

#[derive(Debug, Clone, Copy)]
struct Window {
    count: u64,
    next_reset_ms: u64,
}

/// What a request needs to know about its window.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Decision {
    pub limit: u64,
    pub remaining: u64,
    /// Seconds until the window resets.
    pub reset_seconds: u64,
    pub allowed: bool,
}

/// In-memory limiter shared by all requests of one process.
#[derive(Debug)]
pub struct RateLimiter {
    max: u64,
    window_ms: u64,
    windows: Mutex<HashMap<String, Window>>,
}

impl RateLimiter {
    pub fn new(max: u32, window_ms: u64) -> Self {
        Self {
            max: u64::from(max),
            window_ms,
            windows: Mutex::new(HashMap::new()),
        }
    }

    /// Counts one request from `key` at `now_ms`.
    pub fn hit(&self, key: &str, now_ms: u64) -> Decision {
        let mut windows = self.windows.lock().expect("rate limit lock poisoned");
        if !windows.contains_key(key) && windows.len() >= MAX_TRACKED_CLIENTS {
            windows.retain(|_, w| w.next_reset_ms > now_ms);
            if windows.len() >= MAX_TRACKED_CLIENTS
                && let Some(oldest) = windows
                    .iter()
                    .min_by_key(|(_, w)| w.next_reset_ms)
                    .map(|(k, _)| k.clone())
            {
                windows.remove(&oldest);
            }
        }
        let window = windows
            .entry(key.to_owned())
            .and_modify(|w| {
                if w.next_reset_ms <= now_ms {
                    *w = Window {
                        count: 1,
                        next_reset_ms: now_ms + self.window_ms,
                    };
                } else {
                    w.count += 1;
                }
            })
            .or_insert(Window {
                count: 1,
                next_reset_ms: now_ms + self.window_ms,
            });
        Decision {
            limit: self.max,
            remaining: self.max.saturating_sub(window.count),
            reset_seconds: window.next_reset_ms.saturating_sub(now_ms).div_ceil(1000),
            allowed: window.count <= self.max,
        }
    }
}

/// Layer state: the limiter plus what is needed to resolve the client IP.
#[derive(Clone)]
pub struct RateLimitState {
    pub limiter: Arc<RateLimiter>,
    pub app: AppState,
}

pub async fn rate_limit(State(state): State<RateLimitState>, req: Request, next: Next) -> Response {
    let ip = client_ip_from(
        req.headers(),
        req.extensions(),
        &state.app.config.trusted_proxy_cidrs,
    );
    let decision = state.limiter.hit(&ip, now_ms());

    let mut response = if decision.allowed {
        next.run(req).await
    } else {
        tracing::warn!(client_ip = %ip, "rate limit exceeded");
        ApiError::rate_limit(None).into_response()
    };

    let headers = response.headers_mut();
    let mut set = |name: &'static str, value: u64| {
        if let Ok(value) = HeaderValue::from_str(&value.to_string()) {
            headers.insert(HeaderName::from_static(name), value);
        }
    };
    set("ratelimit-limit", decision.limit);
    set("ratelimit-remaining", decision.remaining);
    set("ratelimit-reset", decision.reset_seconds);
    if !decision.allowed {
        set("retry-after", decision.reset_seconds);
    }
    response
}

fn now_ms() -> u64 {
    u64::try_from(
        SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap_or_default()
            .as_millis(),
    )
    .unwrap_or(u64::MAX)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn counts_within_a_fixed_window_then_blocks() {
        let limiter = RateLimiter::new(2, 60_000);
        let first = limiter.hit("ip", 1_000);
        assert_eq!(
            (first.allowed, first.remaining, first.reset_seconds),
            (true, 1, 60)
        );
        assert!(limiter.hit("ip", 2_000).allowed);
        let third = limiter.hit("ip", 3_000);
        assert_eq!(
            (third.allowed, third.remaining, third.reset_seconds),
            (false, 0, 58)
        );
    }

    #[test]
    fn window_resets_after_the_duration_and_keys_are_independent() {
        let limiter = RateLimiter::new(1, 1_000);
        assert!(limiter.hit("a", 0).allowed);
        assert!(!limiter.hit("a", 500).allowed);
        assert!(limiter.hit("b", 500).allowed);
        assert!(limiter.hit("a", 1_000).allowed);
    }

    #[test]
    fn tracked_clients_are_bounded() {
        let limiter = RateLimiter::new(1, 60_000);
        for i in 0..(MAX_TRACKED_CLIENTS + 10) {
            limiter.hit(&format!("ip{i}"), i as u64);
        }
        assert!(limiter.windows.lock().expect("lock").len() <= MAX_TRACKED_CLIENTS);
    }
}
