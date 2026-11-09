//! `ValidatedQuery<T>`: query string with the Elysia error behaviour.

use std::collections::HashMap;

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use crate::core::error::ApiError;
use crate::core::extract::validate::{Validate, describe_path_error};

/// Deserialises and validates the query string. Unknown parameters are ignored and a repeated
/// parameter keeps its last value, as in Elysia.
///
/// Declare numbers as `Option<`[`Numeric`](super::Numeric)`>` and bound them with
/// [`ValidationErrors::numeric`](super::ValidationErrors::numeric). Failures are 422
/// `VALIDATION_ERROR`. Elysia validates before auth, so put this extractor before
/// [`CurrentUser`](super::CurrentUser) in the handler arguments to keep that order.
///
/// ```ignore
/// async fn list(ValidatedQuery(q): ValidatedQuery<ListQuery>, user: CurrentUser) -> ApiResult<Out> { .. }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedQuery<T>(pub T);

impl<S, T> FromRequestParts<S> for ValidatedQuery<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let query = parts.uri.query().unwrap_or("");
        // serde_urlencoded rejects a repeated field, so collapse to the last occurrence first.
        let last: HashMap<_, _> = form_urlencoded::parse(query.as_bytes()).collect();
        let canonical = form_urlencoded::Serializer::new(String::new())
            .extend_pairs(&last)
            .finish();
        let deserializer =
            serde_urlencoded::Deserializer::new(form_urlencoded::parse(canonical.as_bytes()));
        let value: T = serde_path_to_error::deserialize(deserializer)
            .map_err(|err| ApiError::invalid_input(describe_path_error(&err)))?;
        value
            .validate()
            .map_err(|errors| ApiError::invalid_input(errors.to_string()))?;
        Ok(Self(value))
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::extract::{Numeric, ValidationErrors};
    use axum::http::Request;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Q {
        search: Option<String>,
        limit: Option<Numeric>,
        offset: Option<Numeric>,
    }

    impl Validate for Q {
        fn validate(&self) -> Result<(), ValidationErrors> {
            let mut errors = ValidationErrors::new();
            errors.opt_length("search", self.search.as_deref(), 1..);
            errors.numeric("limit", self.limit, 0.0..=100.0);
            errors.numeric("offset", self.offset, 0.0..);
            errors.into_result()
        }
    }

    async fn extract(uri: &str) -> Result<Q, ApiError> {
        let (mut parts, ()) = Request::builder()
            .uri(uri)
            .body(())
            .expect("request")
            .into_parts();
        ValidatedQuery::<Q>::from_request_parts(&mut parts, &())
            .await
            .map(|q| q.0)
    }

    fn details(err: &ApiError) -> &str {
        err.details().and_then(|d| d.as_str()).expect("details")
    }

    #[tokio::test]
    async fn parses_numbers_and_ignores_unknown_parameters() {
        let q = extract("/x?limit=5&offset=1.5&search=abc&zzz=1")
            .await
            .expect("ok");
        assert_eq!(
            (q.limit, q.offset, q.search.as_deref()),
            (
                Some(Numeric::Value(5.0)),
                Some(Numeric::Value(1.5)),
                Some("abc")
            )
        );
        assert!(extract("/x").await.is_ok());
    }

    #[tokio::test]
    async fn a_repeated_parameter_keeps_its_last_value() {
        let q = extract("/x?limit=abc&limit=7&se%61rch=a&search=b")
            .await
            .expect("ok");
        assert_eq!(
            (q.limit, q.search.as_deref()),
            (Some(Numeric::Value(7.0)), Some("b"))
        );
        assert!(extract("/x?limit=7&limit=abc").await.is_err());
    }

    #[tokio::test]
    async fn errors_follow_the_two_elysia_stages() {
        const BAD: &str = "Property 'limit' should be one of: 'numeric', 'number'";
        for (uri, want) in [
            ("/x?limit=abc", format!("/limit: {BAD}")),
            ("/x?limit=", format!("/limit: {BAD}")),
            (
                "/x?limit=abc&offset=abc",
                format!("/limit: {BAD}, /offset: {}", BAD.replace("limit", "offset")),
            ),
            (
                "/x?search=&limit=abc",
                format!("/search: Expected string length greater or equal to 1, /limit: {BAD}"),
            ),
            // Range errors come second, alone and without a path.
            (
                "/x?limit=200",
                ": Expected number to be less or equal to 100".into(),
            ),
            (
                "/x?offset=-1&limit=200",
                ": Expected number to be less or equal to 100".into(),
            ),
            (
                "/x?limit=-1&offset=-1",
                ": Expected number to be greater or equal to 0".into(),
            ),
            ("/x?limit=Infinity", ": Expected number".into()),
            (
                "/x?search=&limit=200",
                "/search: Expected string length greater or equal to 1".into(),
            ),
            (
                "/x?limit=200&offset=abc",
                format!("/offset: {}", BAD.replace("limit", "offset")),
            ),
        ] {
            let err = extract(uri).await.expect_err(uri);
            assert_eq!(err.status().as_u16(), 422, "{uri}");
            assert_eq!(details(&err), want, "{uri}");
        }
    }
}
