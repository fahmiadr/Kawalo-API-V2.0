//! Safe client IP resolution, ported from `client-ip.util.ts` (quirks included).

use std::net::SocketAddr;

use axum::extract::{ConnectInfo, FromRequestParts};
use axum::http::request::Parts;
use axum::http::{Extensions, HeaderMap};
use regex::Regex;
use std::convert::Infallible;
use std::sync::LazyLock;

use crate::core::clean::parse_int_prefix;
use crate::core::state::AppState;

static IPV4: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^(\d{1,3}\.){3}\d{1,3}$").expect("static regex is valid"));

fn ipv4_to_int(ip: &str) -> u32 {
    ip.split('.').fold(0u32, |acc, octet| {
        let octet = parse_int_prefix(octet).unwrap_or(0);
        // Mirrors `((acc << 8) + octet) >>> 0`: high bits fall off, the sum wraps at 32 bits.
        (acc << 8).wrapping_add(octet as u32)
    })
}

/// Strips the IPv4-mapped IPv6 prefix (`::ffff:`) and surrounding whitespace.
pub fn normalize_ip(raw: &str) -> &str {
    let trimmed = raw.trim();
    trimmed.strip_prefix("::ffff:").unwrap_or(trimmed)
}

/// Whether `ip` lies in `cidr`. Handles IPv4 CIDRs and the IPv6 loopback only; any other IPv6
/// value is compared for equality, as the TypeScript implementation does.
pub fn is_ip_in_cidr(ip: &str, cidr: &str) -> bool {
    let clean_ip = normalize_ip(ip);
    let clean_cidr = cidr.trim();

    if clean_cidr == "::1" || clean_cidr == "::1/128" {
        return clean_ip == "::1" || clean_ip == "0:0:0:0:0:0:0:1";
    }

    let mut parts = clean_cidr.split('/');
    let range = parts.next().unwrap_or("");
    let bits = parts.next().filter(|b| !b.is_empty()).unwrap_or("32");
    let prefix = parse_int_prefix(bits);

    if !IPV4.is_match(clean_ip) || !IPV4.is_match(range) {
        return clean_ip == range;
    }

    let mask = match prefix {
        Some(0) => return true,
        Some(p) if !(0..=32).contains(&p) => return false,
        Some(p @ 1..=31) => u32::MAX << (32 - p),
        // `/32`, and unparsable prefixes (NaN in JavaScript shifts to an all-ones mask).
        _ => u32::MAX,
    };
    ipv4_to_int(clean_ip) & mask == ipv4_to_int(range) & mask
}

/// Whether `ip` belongs to one of the trusted reverse proxy subnets.
pub fn is_trusted_proxy<S: AsRef<str>>(ip: &str, trusted_cidrs: &[S]) -> bool {
    let clean = normalize_ip(ip);
    !clean.is_empty()
        && trusted_cidrs
            .iter()
            .any(|cidr| is_ip_in_cidr(clean, cidr.as_ref()))
}

/// Resolves the real client IP without trusting spoofable headers.
///
/// 1. A socket peer outside `trusted_cidrs` wins outright; `X-Forwarded-For` and `X-Real-IP` are
///    ignored.
/// 2. Behind a trusted proxy, `X-Forwarded-For` is walked right to left and the first hop that is
///    not itself a trusted proxy is the client. If every hop is trusted, the leftmost is used.
/// 3. Otherwise `X-Real-IP`, then the socket address.
///
/// `socket` is `None` only when the peer address is unknown (for example in tests driving the
/// router directly); the loopback address is assumed and headers are trusted.
pub fn resolve_client_ip<S: AsRef<str>>(
    headers: &HeaderMap,
    socket: Option<&str>,
    trusted_cidrs: &[S],
) -> String {
    let raw_socket = socket.filter(|s| !s.is_empty());
    let socket_ip = raw_socket.map_or("127.0.0.1", normalize_ip);

    if raw_socket.is_some() && !is_trusted_proxy(socket_ip, trusted_cidrs) {
        return socket_ip.to_owned();
    }

    if let Some(forwarded) = joined_header(headers, "x-forwarded-for") {
        let hops: Vec<&str> = forwarded
            .split(',')
            .map(normalize_ip)
            .filter(|ip| !ip.is_empty())
            .collect();
        if let Some(client) = hops
            .iter()
            .rev()
            .find(|ip| !is_trusted_proxy(ip, trusted_cidrs))
        {
            return (*client).to_owned();
        }
        if let Some(first) = hops.first() {
            return (*first).to_owned();
        }
    }

    if let Some(real) = joined_header(headers, "x-real-ip") {
        return normalize_ip(&real).to_owned();
    }

    socket_ip.to_owned()
}

/// Header values as `Headers.get` returns them: repeated lines joined with `, `; empty is `None`.
fn joined_header(headers: &HeaderMap, name: &str) -> Option<String> {
    let joined = headers
        .get_all(name)
        .iter()
        .filter_map(|v| v.to_str().ok())
        .collect::<Vec<_>>()
        .join(", ");
    (!joined.is_empty()).then_some(joined)
}

/// The resolved client IP of the current request, using the real socket peer.
///
/// ```ignore
/// async fn login(ClientIp(ip): ClientIp) { /* .. */ }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ClientIp(pub String);

impl FromRequestParts<AppState> for ClientIp {
    type Rejection = Infallible;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        Ok(Self(client_ip_from(
            &parts.headers,
            &parts.extensions,
            &state.config.trusted_proxy_cidrs,
        )))
    }
}

/// Resolves the client IP from the headers and the `ConnectInfo` extension the server inserts.
pub(crate) fn client_ip_from(
    headers: &HeaderMap,
    extensions: &Extensions,
    trusted_cidrs: &[String],
) -> String {
    let socket = extensions
        .get::<ConnectInfo<SocketAddr>>()
        .map(|ConnectInfo(addr)| addr.ip().to_string());
    resolve_client_ip(headers, socket.as_deref(), trusted_cidrs)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::config::Config;

    fn headers(pairs: &[(&'static str, &str)]) -> HeaderMap {
        let mut map = HeaderMap::new();
        for (name, value) in pairs {
            map.insert(*name, value.parse().expect("header value"));
        }
        map
    }

    fn trusted() -> Vec<String> {
        Config::default().trusted_proxy_cidrs
    }

    #[test]
    fn normalize_ip_strips_mapped_prefix() {
        assert_eq!(normalize_ip("::ffff:192.168.1.50"), "192.168.1.50");
        assert_eq!(normalize_ip(" ::ffff:10.0.0.1 "), "10.0.0.1");
        assert_eq!(normalize_ip("192.168.1.1"), "192.168.1.1");
        assert_eq!(normalize_ip("::1"), "::1");
    }

    #[test]
    fn cidr_matches_loopback_and_private_ranges() {
        assert!(is_ip_in_cidr("127.0.0.1", "127.0.0.1/32"));
        assert!(is_ip_in_cidr("127.0.0.2", "127.0.0.0/8"));
        assert!(is_ip_in_cidr("::1", "::1/128"));
        assert!(!is_ip_in_cidr("192.168.1.1", "127.0.0.1/32"));
        assert!(is_ip_in_cidr("10.254.1.2", "10.0.0.0/8"));
        assert!(!is_ip_in_cidr("11.0.0.1", "10.0.0.0/8"));
        assert!(is_ip_in_cidr("172.18.0.5", "172.16.0.0/12"));
        assert!(!is_ip_in_cidr("172.33.0.1", "172.16.0.0/12"));
        assert!(is_ip_in_cidr("192.168.1.100", "192.168.0.0/16"));
        assert!(!is_ip_in_cidr("192.169.1.1", "192.168.0.0/16"));
    }

    #[test]
    fn cidr_edge_cases_follow_the_typescript_quirks() {
        assert!(is_ip_in_cidr("8.8.8.8", "0.0.0.0/0"));
        assert!(!is_ip_in_cidr("8.8.8.8", "8.8.8.8/33"));
        assert!(is_ip_in_cidr("8.8.8.8", "8.8.8.8"));
        assert!(!is_ip_in_cidr("8.8.8.9", "8.8.8.8/x"));
        assert!(is_ip_in_cidr("2001:db8::1", "2001:db8::1"));
        assert!(!is_ip_in_cidr("2001:db8::2", "2001:db8::1/64"));
    }

    #[test]
    fn trusted_proxy_uses_the_given_subnets() {
        let cidrs = trusted();
        assert!(is_trusted_proxy("127.0.0.1", &cidrs));
        assert!(is_trusted_proxy("10.0.0.1", &cidrs));
        assert!(is_trusted_proxy("172.20.0.2", &cidrs));
        assert!(!is_trusted_proxy("203.0.113.195", &cidrs));
        assert!(!is_trusted_proxy("", &cidrs));
    }

    #[test]
    fn untrusted_socket_ignores_forwarding_headers() {
        let h = headers(&[
            ("x-forwarded-for", "8.8.8.8, 1.1.1.1"),
            ("x-real-ip", "8.8.8.8"),
        ]);
        assert_eq!(
            resolve_client_ip(&h, Some("203.0.113.5"), &trusted()),
            "203.0.113.5"
        );
    }

    #[test]
    fn forwarded_for_walks_right_to_left_past_trusted_hops() {
        let h = headers(&[("x-forwarded-for", "182.253.120.44, 10.0.0.5")]);
        assert_eq!(
            resolve_client_ip(&h, Some("127.0.0.1"), &trusted()),
            "182.253.120.44"
        );

        let spoofed = headers(&[("x-forwarded-for", "1.1.1.1, 203.0.113.50, 10.0.0.2")]);
        assert_eq!(
            resolve_client_ip(&spoofed, Some("172.18.0.2"), &trusted()),
            "203.0.113.50"
        );
    }

    #[test]
    fn all_trusted_hops_fall_back_to_the_leftmost() {
        let h = headers(&[("x-forwarded-for", "10.0.0.9, 10.0.0.5")]);
        assert_eq!(
            resolve_client_ip(&h, Some("127.0.0.1"), &trusted()),
            "10.0.0.9"
        );
    }

    #[test]
    fn falls_back_to_real_ip_then_socket() {
        let real = headers(&[("x-real-ip", "114.122.50.88")]);
        assert_eq!(
            resolve_client_ip(&real, Some("127.0.0.1"), &trusted()),
            "114.122.50.88"
        );
        assert_eq!(
            resolve_client_ip(&HeaderMap::new(), Some("::ffff:127.0.0.1"), &trusted()),
            "127.0.0.1"
        );
    }

    #[test]
    fn mapped_socket_is_normalised_and_spoofing_rejected() {
        let h = headers(&[
            ("x-forwarded-for", "127.0.0.1, 192.168.1.1"),
            ("x-real-ip", "127.0.0.1"),
        ]);
        assert_eq!(
            resolve_client_ip(&h, Some("::ffff:203.0.113.88"), &trusted()),
            "203.0.113.88"
        );
    }

    #[test]
    fn unknown_socket_defaults_to_loopback() {
        assert_eq!(
            resolve_client_ip(&HeaderMap::new(), None, &trusted()),
            "127.0.0.1"
        );
    }
}
