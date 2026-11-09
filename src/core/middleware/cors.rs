//! CORS exactly as `@elysiajs/cors` emitted it for the TypeScript app.
//!
//! Every response carries the static allow/expose headers and credentials; the origin headers
//! depend on `CORS_ORIGIN` (`*` mirrors the request origin, a comma list is matched against it).
//! Preflight requests are answered with an empty 204 before rate limiting.

use std::sync::Arc;

use axum::extract::{Request, State};
use axum::http::{HeaderMap, HeaderName, HeaderValue, Method, StatusCode, header};
use axum::middleware::Next;
use axum::response::Response;

const ALLOW_METHODS: &str = "GET, POST, PUT, PATCH, DELETE, OPTIONS";
const ALLOW_HEADERS: &str = "Content-Type, Authorization, x-request-id";
const EXPOSE_HEADERS: &str = "x-request-id";
const PREFLIGHT_MAX_AGE: &str = "5";

/// Parsed `CORS_ORIGIN`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Origins {
    /// `*`: reflect whatever origin the request names.
    Reflect,
    /// Explicit list; entries are compared against the origin with and without its scheme. The
    /// entries are not trimmed, as in the TypeScript code.
    List(Vec<String>),
}

impl Origins {
    pub fn parse(cors_origin: &str) -> Self {
        if cors_origin == "*" {
            Self::Reflect
        } else {
            Self::List(cors_origin.split(',').map(str::to_owned).collect())
        }
    }

    /// `(vary, access-control-allow-origin)` for a request origin.
    fn headers_for(&self, origin: Option<&str>) -> (Option<&'static str>, Option<String>) {
        match self {
            Self::Reflect => (Some("*"), Some(origin.unwrap_or("*").to_owned())),
            Self::List(list) if list.iter().any(|o| o == "*") => (Some("*"), Some("*".to_owned())),
            Self::List(list) if list.is_empty() => (None, None),
            Self::List(list) => {
                let from = origin.unwrap_or("");
                let bare = from.split_once("://").map_or(from, |(_, rest)| rest);
                if list.iter().any(|o| o == from || o == bare) {
                    (
                        Some("Origin"),
                        Some(if from.is_empty() { "*" } else { from }.to_owned()),
                    )
                } else {
                    (Some("Origin"), None)
                }
            }
        }
    }
}

pub async fn cors(State(origins): State<Arc<Origins>>, req: Request, next: Next) -> Response {
    let request_origin = req
        .headers()
        .get(header::ORIGIN)
        .and_then(|v| v.to_str().ok())
        .map(str::to_owned);
    let preflight = req.method() == Method::OPTIONS;
    let wants_method = req
        .headers()
        .contains_key(header::ACCESS_CONTROL_REQUEST_METHOD);

    let mut response = if preflight {
        let mut response = Response::new(axum::body::Body::empty());
        *response.status_mut() = StatusCode::NO_CONTENT;
        response.headers_mut().insert(
            header::ACCESS_CONTROL_MAX_AGE,
            HeaderValue::from_static(PREFLIGHT_MAX_AGE),
        );
        response
    } else {
        next.run(req).await
    };

    let headers = response.headers_mut();
    let (vary, allow_origin) = origins.headers_for(request_origin.as_deref());
    if let Some(vary) = vary {
        headers.insert(header::VARY, HeaderValue::from_static(vary));
    }
    if let Some(value) = allow_origin.and_then(|v| HeaderValue::from_str(&v).ok()) {
        headers.insert(header::ACCESS_CONTROL_ALLOW_ORIGIN, value);
    }
    if !preflight || wants_method {
        set_static(headers, header::ACCESS_CONTROL_ALLOW_METHODS, ALLOW_METHODS);
    }
    set_static(headers, header::ACCESS_CONTROL_ALLOW_HEADERS, ALLOW_HEADERS);
    set_static(
        headers,
        header::ACCESS_CONTROL_EXPOSE_HEADERS,
        EXPOSE_HEADERS,
    );
    set_static(headers, header::ACCESS_CONTROL_ALLOW_CREDENTIALS, "true");
    response
}

fn set_static(headers: &mut HeaderMap, name: HeaderName, value: &'static str) {
    headers.insert(name, HeaderValue::from_static(value));
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn star_reflects_the_request_origin() {
        let origins = Origins::parse("*");
        assert_eq!(
            origins.headers_for(Some("http://x.com")),
            (Some("*"), Some("http://x.com".into()))
        );
        assert_eq!(origins.headers_for(None), (Some("*"), Some("*".into())));
    }

    #[test]
    fn list_matches_with_or_without_scheme() {
        let origins = Origins::parse("app.kawalo.id,https://admin.kawalo.id");
        assert_eq!(
            origins.headers_for(Some("https://app.kawalo.id")),
            (Some("Origin"), Some("https://app.kawalo.id".into()))
        );
        assert_eq!(
            origins.headers_for(Some("https://admin.kawalo.id")),
            (Some("Origin"), Some("https://admin.kawalo.id".into()))
        );
        assert_eq!(
            origins.headers_for(Some("https://evil.com")),
            (Some("Origin"), None)
        );
        assert_eq!(origins.headers_for(None), (Some("Origin"), None));
    }
}
