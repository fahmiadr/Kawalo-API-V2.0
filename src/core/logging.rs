//! Structured logging setup.

use tracing_subscriber::EnvFilter;

use crate::core::config::Config;

/// Installs the global subscriber: human-readable lines in development, JSON in production.
///
/// `LOG_LEVEL` accepts the pino names (`fatal` maps to `error`, `silent` turns logging off) and
/// applies to this crate; dependencies stay at `warn`. `RUST_LOG`, when set, takes over.
///
/// # Errors
/// When a global subscriber is already installed.
pub fn init(config: &Config) -> Result<(), tracing_subscriber::util::TryInitError> {
    use tracing_subscriber::layer::SubscriberExt;
    use tracing_subscriber::util::SubscriberInitExt;

    let filter = EnvFilter::try_from_default_env()
        .unwrap_or_else(|_| EnvFilter::new(default_directives(&config.log_level)));
    let registry = tracing_subscriber::registry().with(filter);
    if config.is_production() {
        registry
            .with(tracing_subscriber::fmt::layer().json())
            .try_init()
    } else {
        registry
            .with(
                tracing_subscriber::fmt::layer()
                    .compact()
                    .with_target(false),
            )
            .try_init()
    }
}

fn default_directives(log_level: &str) -> String {
    let level = match log_level.trim().to_ascii_lowercase().as_str() {
        "fatal" => "error".to_owned(),
        "silent" => "off".to_owned(),
        other => other.to_owned(),
    };
    format!("warn,kawalo_api={level}")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pino_levels_map_to_tracing_directives() {
        assert_eq!(default_directives("debug"), "warn,kawalo_api=debug");
        assert_eq!(default_directives("FATAL"), "warn,kawalo_api=error");
        assert_eq!(default_directives("silent"), "warn,kawalo_api=off");
    }
}
