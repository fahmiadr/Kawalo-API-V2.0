//! `ValidatedQuery<T>`: query string with the Elysia error behaviour.

use axum::extract::FromRequestParts;
use axum::http::request::Parts;
use serde::de::DeserializeOwned;

use crate::core::error::ApiError;
use crate::core::extract::validate::{Validate, describe_path_error};

/// Deserialises and validates the query string. Unknown parameters are ignored.
///
/// Numbers arrive as text and are parsed like Elysia's `t.Numeric`: declare them as
/// `Option<f64>` and bound them with [`ValidationErrors::range`](super::ValidationErrors::range).
/// Failures are 422 `VALIDATION_ERROR`. Elysia validates before auth, so put this extractor
/// before [`CurrentUser`](super::CurrentUser) in the handler arguments to keep that order.
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
        let pairs = form_urlencoded::parse(query.as_bytes());
        let deserializer = serde_urlencoded::Deserializer::new(pairs);
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
    use crate::core::extract::ValidationErrors;
    use axum::http::Request;
    use serde::Deserialize;

    #[derive(Debug, Deserialize)]
    struct Q {
        search: Option<String>,
        limit: Option<f64>,
        offset: Option<f64>,
    }

    impl Validate for Q {
        fn validate(&self) -> Result<(), ValidationErrors> {
            let mut errors = ValidationErrors::new();
            errors.opt_length("search", self.search.as_deref(), 1..);
            errors.opt_range("limit", self.limit, 0.0..=100.0);
            errors.opt_range("offset", self.offset, 0.0..);
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

    #[tokio::test]
    async fn parses_numbers_and_ignores_unknown_parameters() {
        let q = extract("/x?limit=5&offset=1.5&search=abc&zzz=1")
            .await
            .expect("ok");
        assert_eq!(
            (q.limit, q.offset, q.search.as_deref()),
            (Some(5.0), Some(1.5), Some("abc"))
        );
        assert!(extract("/x").await.is_ok());
    }

    #[tokio::test]
    async fn bad_values_are_422_with_a_field_path() {
        for (uri, detail) in [
            ("/x?limit=abc", "/limit:"),
            ("/x?limit=", "/limit:"),
            (
                "/x?limit=200",
                "/limit: Expected number to be less or equal to 100",
            ),
            (
                "/x?offset=-1",
                "/offset: Expected number to be greater or equal to 0",
            ),
            (
                "/x?search=",
                "/search: Expected string length greater or equal to 1",
            ),
        ] {
            let err = extract(uri).await.expect_err(uri);
            assert_eq!(err.status().as_u16(), 422, "{uri}");
            let details = err.details().and_then(|d| d.as_str()).expect("details");
            assert!(details.starts_with(detail), "{uri}: {details}");
        }
    }
}
