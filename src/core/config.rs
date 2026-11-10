//! Environment configuration, ported from `env.config.ts` and `auth.config.ts`.

use std::env;
use std::path::{Path, PathBuf};
use std::sync::OnceLock;
use std::time::{Duration, SystemTime};

use crate::core::clean::parse_int_prefix;

const DEFAULT_TRUSTED_PROXY_CIDRS: &str =
    "127.0.0.1/32,::1/128,10.0.0.0/8,172.16.0.0/12,192.168.0.0/16";

const DEFAULT_UPLOAD_DIR: &str = "./uploads";

/// Settings for the authentication domain (`AuthConfig` in the TypeScript code).
#[derive(Debug, Clone)]
pub struct AuthConfig {
    /// Refresh token lifetime in seconds; also the Redis TTL of the session version key.
    pub refresh_token_expires_in: u64,
    pub login_rate_limit_ip_max: u32,
    /// Seconds.
    pub login_rate_limit_duration: u64,
    pub login_max_failed_attempts: u32,
    /// Seconds.
    pub login_lockout_duration: u64,
}

impl AuthConfig {
    pub fn from_env() -> Self {
        Self {
            refresh_token_expires_in: env_uint("REFRESH_TOKEN_EXPIRES_IN", 1_209_600),
            login_rate_limit_ip_max: env_uint("LOGIN_RATE_LIMIT_IP_MAX", 10),
            login_rate_limit_duration: env_uint("LOGIN_RATE_LIMIT_DURATION", 900),
            login_max_failed_attempts: env_uint("LOGIN_MAX_FAILED_ATTEMPTS", 5),
            login_lockout_duration: env_uint("LOGIN_LOCKOUT_DURATION", 900),
        }
    }

    /// Expiry instant of a refresh token issued at `now`.
    pub fn refresh_token_expiry(&self, now: SystemTime) -> SystemTime {
        now + Duration::from_secs(self.refresh_token_expires_in)
    }
}

impl Default for AuthConfig {
    fn default() -> Self {
        Self {
            refresh_token_expires_in: 1_209_600,
            login_rate_limit_ip_max: 10,
            login_rate_limit_duration: 900,
            login_max_failed_attempts: 5,
            login_lockout_duration: 900,
        }
    }
}

/// Process-wide settings, all keys and defaults identical to `EnvConfig`.
#[derive(Clone)]
pub struct Config {
    /// Lower-cased `NODE_ENV`, `development` when unset.
    pub node_env: String,
    pub port: u16,
    pub log_level: String,
    pub database_url: String,
    /// Kept for key parity. sqlx has no global text-protocol switch; use `sqlx::raw_sql` where
    /// the binary protocol misbehaves.
    pub database_use_text_protocol: bool,
    pub redis_url: String,
    /// Access token lifetime in seconds.
    pub jwt_expires_in: u64,
    pub cors_origin: String,
    pub rate_limit_max: u32,
    /// Fixed window length in milliseconds.
    pub rate_limit_duration: u64,
    pub trusted_proxy_cidrs: Vec<String>,
    /// Root of the stored uploads; must be a persistent volume in production.
    pub upload_dir: PathBuf,
    /// Origin of the URLs handed out for uploads, without a trailing slash. Empty derives it from
    /// the request `Host` header (development only).
    pub public_base_url: String,
    pub auth: AuthConfig,
}

impl Config {
    /// Loads `.env.dev|staging|prod` (chosen by `NODE_ENV`) and then `.env` from the working
    /// directory, without overriding variables that are already set, then reads the environment.
    pub fn from_env() -> Self {
        load_dotenv_files();
        let node_env = raw_node_env();
        let log_level = env::var("LOG_LEVEL")
            .ok()
            .filter(|v| !v.is_empty())
            .unwrap_or_else(|| {
                if node_env == "production" {
                    "info"
                } else {
                    "debug"
                }
                .to_owned()
            });
        Self {
            port: env_uint("PORT", 3000),
            log_level,
            database_url: env_string("DATABASE_URL", "mysql://root:@localhost:3306/kawalo"),
            database_use_text_protocol: env_bool("DATABASE_USE_TEXT_PROTOCOL", false),
            redis_url: env_string("REDIS_URL", "redis://localhost:6379"),
            jwt_expires_in: env_uint("JWT_EXPIRES_IN", 900),
            cors_origin: env_string("CORS_ORIGIN", "*"),
            rate_limit_max: env_uint("RATE_LIMIT_MAX", 100),
            rate_limit_duration: env_uint("RATE_LIMIT_DURATION", 60_000),
            trusted_proxy_cidrs: env_list("TRUSTED_PROXY_CIDRS", DEFAULT_TRUSTED_PROXY_CIDRS),
            upload_dir: PathBuf::from(env_string("UPLOAD_DIR", DEFAULT_UPLOAD_DIR)),
            public_base_url: env_string("PUBLIC_BASE_URL", "")
                .trim()
                .trim_end_matches('/')
                .to_owned(),
            auth: AuthConfig::from_env(),
            node_env,
        }
    }

    /// The configuration read from the environment on first use.
    ///
    /// Only for code that has no access to [`AppState`](crate::core::state::AppState), such as
    /// `IntoResponse` impls deciding whether to expose error details.
    pub fn global() -> &'static Config {
        static GLOBAL: OnceLock<Config> = OnceLock::new();
        GLOBAL.get_or_init(Config::from_env)
    }

    pub fn is_production(&self) -> bool {
        self.node_env == "production"
    }

    pub fn is_development(&self) -> bool {
        self.node_env == "development"
    }

    pub fn is_staging(&self) -> bool {
        self.node_env == "staging"
    }

    pub fn is_test(&self) -> bool {
        self.node_env == "test"
    }
}

/// Connection URLs carry credentials, so `Debug` redacts them.
impl std::fmt::Debug for Config {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Config")
            .field("node_env", &self.node_env)
            .field("port", &self.port)
            .field("log_level", &self.log_level)
            .field("database_url", &"<redacted>")
            .field(
                "database_use_text_protocol",
                &self.database_use_text_protocol,
            )
            .field("redis_url", &"<redacted>")
            .field("jwt_expires_in", &self.jwt_expires_in)
            .field("cors_origin", &self.cors_origin)
            .field("rate_limit_max", &self.rate_limit_max)
            .field("rate_limit_duration", &self.rate_limit_duration)
            .field("trusted_proxy_cidrs", &self.trusted_proxy_cidrs)
            .field("upload_dir", &self.upload_dir)
            .field("public_base_url", &self.public_base_url)
            .field("auth", &self.auth)
            .finish()
    }
}

impl Default for Config {
    /// Defaults only; ignores the environment. Intended for tests that override single fields.
    fn default() -> Self {
        Self {
            node_env: "test".to_owned(),
            port: 3000,
            log_level: "debug".to_owned(),
            database_url: "mysql://root:@localhost:3306/kawalo".to_owned(),
            database_use_text_protocol: false,
            redis_url: "redis://localhost:6379".to_owned(),
            jwt_expires_in: 900,
            cors_origin: "*".to_owned(),
            rate_limit_max: 100,
            rate_limit_duration: 60_000,
            trusted_proxy_cidrs: split_csv(DEFAULT_TRUSTED_PROXY_CIDRS),
            upload_dir: PathBuf::from(DEFAULT_UPLOAD_DIR),
            public_base_url: String::new(),
            auth: AuthConfig::default(),
        }
    }
}

fn raw_node_env() -> String {
    env::var("NODE_ENV")
        .ok()
        .map(|v| v.trim().to_lowercase())
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| "development".to_owned())
}

fn load_dotenv_files() {
    let file = match raw_node_env().as_str() {
        "production" => ".env.prod",
        "staging" => ".env.staging",
        _ => ".env.dev",
    };
    let Ok(cwd) = env::current_dir() else { return };
    // Missing files are expected; dotenvy never overrides variables that are already set.
    for name in [file, ".env"] {
        let _ = dotenvy::from_path(Path::new(&cwd).join(name));
    }
}

fn env_string(key: &str, default: &str) -> String {
    env::var(key)
        .ok()
        .filter(|v| !v.is_empty())
        .unwrap_or_else(|| default.to_owned())
}

/// Like JavaScript `Number.parseInt(raw, 10)`: takes the leading integer, falls back to the
/// default when empty, unparsable or negative for an unsigned target.
fn env_uint<T: TryFrom<i64>>(key: &str, default: T) -> T {
    let Ok(raw) = env::var(key) else {
        return default;
    };
    parse_int_prefix(&raw)
        .and_then(|n| T::try_from(n).ok())
        .unwrap_or(default)
}

fn env_bool(key: &str, default: bool) -> bool {
    match env::var(key) {
        Ok(raw) if !raw.trim().is_empty() => {
            matches!(raw.trim().to_lowercase().as_str(), "true" | "1" | "yes")
        }
        _ => default,
    }
}

fn env_list(key: &str, default_csv: &str) -> Vec<String> {
    let raw = env::var(key).ok().filter(|v| !v.is_empty());
    split_csv(raw.as_deref().unwrap_or(default_csv))
}

fn split_csv(csv: &str) -> Vec<String> {
    csv.split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(str::to_owned)
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn csv_is_trimmed_and_filtered() {
        assert_eq!(split_csv(" a , ,b,"), vec!["a", "b"]);
    }

    #[test]
    fn debug_output_redacts_connection_urls() {
        let config = Config {
            database_url: "mysql://root:s3cret@db/kawalo".to_owned(),
            ..Config::default()
        };
        assert!(!format!("{config:?}").contains("s3cret"));
    }

    #[test]
    fn default_matches_documented_defaults() {
        let cfg = Config::default();
        assert_eq!(cfg.rate_limit_max, 100);
        assert_eq!(cfg.rate_limit_duration, 60_000);
        assert_eq!(cfg.trusted_proxy_cidrs.len(), 5);
        assert_eq!(cfg.auth.refresh_token_expires_in, 1_209_600);
    }
}
