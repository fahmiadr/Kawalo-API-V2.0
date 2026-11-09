//! Edges of the router that only Elysia's behaviour dictates: path normalisation and `allow`.

use axum::extract::Request;
use axum::http::uri::PathAndQuery;
use axum::http::{Uri, header};
use axum::response::Response;

/// Drops one trailing slash from the path (`/health/` serves `/health`), as Elysia's router does.
/// `/`, `//` and a second trailing slash are left alone and stay 404.
pub fn trim_trailing_slash(mut req: Request) -> Request {
    let path = req.uri().path();
    if path.len() > 1
        && path.ends_with('/')
        && let Some(uri) = without_trailing_slash(req.uri())
    {
        *req.uri_mut() = uri;
    }
    req
}

/// Removes `allow`, which axum's method routers add to a 404 for a wrong method after every layer
/// has run; Elysia sends none. Has to wrap the whole router for the same reason.
pub fn strip_allow(mut response: Response) -> Response {
    response.headers_mut().remove(header::ALLOW);
    response
}

fn without_trailing_slash(uri: &Uri) -> Option<Uri> {
    let path = uri.path().strip_suffix('/')?;
    let rewritten = match uri.query() {
        Some(query) => format!("{path}?{query}"),
        None => path.to_owned(),
    };
    let mut parts = uri.clone().into_parts();
    parts.path_and_query = Some(PathAndQuery::try_from(rewritten).ok()?);
    Uri::from_parts(parts).ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn trimmed(uri: &str) -> String {
        let req = Request::builder().uri(uri).body(()).expect("request");
        let (parts, ()) = req.into_parts();
        let req = Request::from_parts(parts, axum::body::Body::empty());
        trim_trailing_slash(req).uri().to_string()
    }

    #[test]
    fn strips_exactly_one_trailing_slash_and_keeps_the_query() {
        for (input, want) in [
            ("/health/", "/health"),
            ("/health/?a=1&b=2/", "/health?a=1&b=2/"),
            (
                "/api/v1/guests/master-types/",
                "/api/v1/guests/master-types",
            ),
            ("/health//", "/health/"),
            ("/", "/"),
            ("//", "/"),
            ("/health", "/health"),
            ("/health?x=/", "/health?x=/"),
        ] {
            assert_eq!(trimmed(input), want, "{input}");
        }
    }
}
