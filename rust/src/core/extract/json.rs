//! `ValidatedJson<T>`: JSON request body with the Elysia error behaviour.

use axum::extract::{FromRequest, Request};
use axum::http::{StatusCode, header};
use bytes::Bytes;
use serde::de::DeserializeOwned;

use crate::core::error::{ApiError, ErrorCode};
use crate::core::extract::validate::{Validate, describe_path_error};

/// Deserialises and validates a JSON body. Unknown fields are ignored, as Elysia strips them.
///
/// Failures match the TypeScript server:
/// * malformed or empty JSON with a JSON content type: 400 `BAD_REQUEST`
/// * schema mismatch, failed [`Validate`], or a body that is not JSON at all (including a missing
///   body): 422 `VALIDATION_ERROR`
///
/// ```ignore
/// async fn create(user: CurrentUser, ValidatedJson(body): ValidatedJson<CreateDto>) -> ApiResult<Out> { .. }
/// ```
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct ValidatedJson<T>(pub T);

impl<S, T> FromRequest<S> for ValidatedJson<T>
where
    S: Send + Sync,
    T: DeserializeOwned + Validate,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &S) -> Result<Self, Self::Rejection> {
        let is_json = req
            .headers()
            .get(header::CONTENT_TYPE)
            .and_then(|v| v.to_str().ok())
            .is_some_and(is_json_media_type);

        let bytes = Bytes::from_request(req, state).await.map_err(|rejection| {
            let status = rejection.status();
            if status == StatusCode::PAYLOAD_TOO_LARGE {
                ApiError::new(status, ErrorCode::BadRequest, "Ukuran data terlalu besar")
            } else {
                ApiError::malformed_json()
            }
        })?;

        if !is_json {
            return Err(ApiError::invalid_input(": Expected object"));
        }
        if bytes.iter().all(u8::is_ascii_whitespace) {
            return Err(ApiError::malformed_json());
        }

        let mut deserializer = serde_json::Deserializer::from_slice(&bytes);
        let value: T = serde_path_to_error::deserialize(&mut deserializer).map_err(|err| {
            use serde_json::error::Category;
            match err.inner().classify() {
                Category::Syntax | Category::Eof | Category::Io => ApiError::malformed_json(),
                Category::Data => ApiError::invalid_input(describe_path_error(&err)),
            }
        })?;
        // `from_slice` stops at the end of the first value; trailing garbage is malformed JSON.
        deserializer.end().map_err(|_| ApiError::malformed_json())?;

        value
            .validate()
            .map_err(|errors| ApiError::invalid_input(errors.to_string()))?;
        Ok(Self(value))
    }
}

fn is_json_media_type(content_type: &str) -> bool {
    let media = content_type
        .split(';')
        .next()
        .unwrap_or("")
        .trim()
        .to_ascii_lowercase();
    media == "application/json" || media.ends_with("+json")
}

#[cfg(test)]
mod tests {
    use super::*;
    use axum::body::Body;
    use http_body_util::BodyExt;
    use serde::Deserialize;
    use serde_json::{Value, json};

    #[derive(Debug, Deserialize)]
    struct Dto {
        email: String,
        note: Option<String>,
    }

    impl Validate for Dto {
        fn validate(&self) -> Result<(), crate::core::extract::ValidationErrors> {
            let mut errors = crate::core::extract::ValidationErrors::new();
            errors.email("email", &self.email);
            errors.into_result()
        }
    }

    async fn extract(content_type: Option<&str>, body: &str) -> Result<Dto, (StatusCode, Value)> {
        let mut builder = Request::builder().method("POST").uri("/");
        if let Some(ct) = content_type {
            builder = builder.header(header::CONTENT_TYPE, ct);
        }
        let req = builder.body(Body::from(body.to_owned())).expect("request");
        match ValidatedJson::<Dto>::from_request(req, &()).await {
            Ok(ValidatedJson(dto)) => Ok(dto),
            Err(err) => {
                let response = axum::response::IntoResponse::into_response(err);
                let status = response.status();
                let bytes = response
                    .into_body()
                    .collect()
                    .await
                    .expect("body")
                    .to_bytes();
                Err((status, serde_json::from_slice(&bytes).expect("json")))
            }
        }
    }

    const JSON: Option<&str> = Some("application/json");

    #[tokio::test]
    async fn valid_body_with_unknown_fields_passes() {
        let dto = extract(JSON, r#"{"email":"a@b.co","zzz":1}"#)
            .await
            .expect("ok");
        assert_eq!((dto.email.as_str(), dto.note), ("a@b.co", None));
        assert!(
            extract(
                Some("application/json; charset=utf-8"),
                r#"{"email":"a@b.co"}"#
            )
            .await
            .is_ok()
        );
    }

    #[tokio::test]
    async fn malformed_and_empty_json_are_400() {
        for body in ["{bad", "", "  ", r#"{"email":"a@b.co"} x"#] {
            let (status, json) = extract(JSON, body).await.expect_err(body);
            assert_eq!(status, StatusCode::BAD_REQUEST, "{body:?}");
            assert_eq!(json["error_code"], "BAD_REQUEST");
            assert_eq!(json["message"], "Format data JSON tidak valid");
        }
    }

    #[tokio::test]
    async fn schema_and_constraint_failures_are_422_with_details() {
        let (status, json) = extract(JSON, "{}").await.expect_err("missing");
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(json["error_code"], "VALIDATION_ERROR");
        assert_eq!(json["message"], "Format data masukan tidak valid");
        assert_eq!(json["details"], "/email: Expected required property");

        let (_, json) = extract(JSON, r#"{"email":"nope"}"#)
            .await
            .expect_err("format");
        assert_eq!(json["details"], "/email: Property 'email' should be email");

        let (_, json) = extract(JSON, r#"{"email":5}"#).await.expect_err("type");
        assert!(
            json["details"]
                .as_str()
                .expect("details")
                .starts_with("/email: invalid type")
        );

        let (status, json) = extract(JSON, "null").await.expect_err("null");
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(json["error_code"], json!("VALIDATION_ERROR"));
    }

    #[tokio::test]
    async fn non_json_or_missing_content_type_is_422() {
        for ct in [None, Some("text/plain")] {
            let (status, json) = extract(ct, r#"{"email":"a@b.co"}"#)
                .await
                .expect_err("non-json");
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
            assert_eq!(json["details"], ": Expected object");
        }
        assert_eq!(
            extract(None, "").await.expect_err("empty").0,
            StatusCode::UNPROCESSABLE_ENTITY
        );
    }
}
