//! Response envelope, ported from `api-response.ts`.
//!
//! Success: `{"success":true,"message":..,"data":..}`. Failure:
//! `{"success":false,"message":..,"error_code":..,"details":..}` (see [`ApiError`]).
//!
//! ```
//! use kawalo_api::core::response::{ApiResult, ok};
//!
//! async fn handler() -> ApiResult<Vec<u32>> {
//!     ok(vec![1, 2], "Berhasil")
//! }
//! ```

use axum::Json;
use axum::http::StatusCode;
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::Value;

use crate::core::config::Config;
use crate::core::error::{ApiError, ErrorCode};

/// Standard handler result.
pub type ApiResult<T> = Result<Success<T>, ApiError>;

/// Body of a successful response.
#[derive(Debug, Clone, Serialize)]
pub struct SuccessBody<T> {
    pub success: bool,
    pub message: String,
    pub data: T,
}

/// Body of a failed response; `details` is always absent in production.
#[derive(Debug, Clone, Serialize)]
pub struct ErrorBody {
    pub success: bool,
    pub message: String,
    pub error_code: ErrorCode,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub details: Option<Value>,
    /// Extras for the app, kept in production (additive to the legacy envelope).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub data: Option<Value>,
}

impl ErrorBody {
    pub fn new(message: impl Into<String>, error_code: ErrorCode, details: Option<Value>) -> Self {
        let details = if Config::global().is_production() {
            None
        } else {
            details
        };
        Self {
            success: false,
            message: message.into(),
            error_code,
            details,
            data: None,
        }
    }
}

/// A success envelope with a chosen status; build it with [`ok`] or [`created`].
#[derive(Debug, Clone)]
pub struct Success<T> {
    status: StatusCode,
    body: SuccessBody<T>,
}

impl<T> Success<T> {
    pub fn with_status(status: StatusCode, data: T, message: impl Into<String>) -> Self {
        Self {
            status,
            body: SuccessBody {
                success: true,
                message: message.into(),
                data,
            },
        }
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn data(&self) -> &T {
        &self.body.data
    }
}

impl<T: Serialize> IntoResponse for Success<T> {
    fn into_response(self) -> Response {
        (self.status, Json(self.body)).into_response()
    }
}

/// 200 OK envelope.
///
/// # Errors
/// Never fails; the `Result` lets handlers return it directly.
pub fn ok<T>(data: T, message: impl Into<String>) -> ApiResult<T> {
    Ok(Success::with_status(StatusCode::OK, data, message))
}

/// 201 Created envelope.
///
/// # Errors
/// Never fails; the `Result` lets handlers return it directly.
pub fn created<T>(data: T, message: impl Into<String>) -> ApiResult<T> {
    Ok(Success::with_status(StatusCode::CREATED, data, message))
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;
    use serde_json::json;

    async fn body_json(response: Response) -> Value {
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        serde_json::from_slice(&bytes).expect("json body")
    }

    #[tokio::test]
    async fn success_envelope_shape() {
        let response = ok(json!({"a": 1}), "Berhasil").expect("ok").into_response();
        assert_eq!(response.status(), StatusCode::OK);
        assert_eq!(
            body_json(response).await,
            json!({"success": true, "message": "Berhasil", "data": {"a": 1}})
        );
    }

    #[tokio::test]
    async fn created_uses_201_and_unit_data_is_null() {
        let response = created((), "Dibuat").expect("created").into_response();
        assert_eq!(response.status(), StatusCode::CREATED);
        assert_eq!(body_json(response).await["data"], Value::Null);
    }

    #[test]
    fn error_body_key_order_matches_typescript() {
        let body = ErrorBody::new("m", ErrorCode::BadRequest, Some(json!("d")));
        assert_eq!(
            serde_json::to_string(&body).expect("serialise"),
            r#"{"success":false,"message":"m","error_code":"BAD_REQUEST","details":"d"}"#
        );
    }
}
