//! Application-wide constants, ported from `app.constants.ts`.

pub const APP_NAME: &str = "Kawalo Mobile API";
pub const API_BASE_PATH: &str = "/api";
pub const DEFAULT_API_VERSION: &str = "v1";
pub const DEFAULT_PAGE_SIZE: u32 = 20;
pub const MAX_PAGE_SIZE: u32 = 100;
pub const AUTH_TOKEN_TYPE: &str = "Bearer";
pub const JWT_ISSUER: &str = "kawalo_auth";
pub const JWT_AUDIENCE: &str = "kawalo_mobile";

/// Route prefix of an API version: `api_version("v2")` and `api_version("2")` both give `/api/v2`.
pub fn api_version(version: &str) -> String {
    let target = version.trim();
    let clean = if target.starts_with('v') {
        target.to_owned()
    } else {
        format!("v{target}")
    };
    format!("{API_BASE_PATH}/{clean}")
}

/// Prefix of the default API version, `/api/v1`.
pub fn default_api_prefix() -> String {
    api_version(DEFAULT_API_VERSION)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn api_version_normalises_input() {
        assert_eq!(api_version("v2"), "/api/v2");
        assert_eq!(api_version(" 3 "), "/api/v3");
        assert_eq!(default_api_prefix(), "/api/v1");
    }
}
