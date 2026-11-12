//! `ApiError`, the single error type of every handler; ported from `failure.ts` and
//! `error.codes.ts`.
//!
//! Handlers return [`ApiResult`](crate::core::response::ApiResult). `?` converts sqlx and Redis
//! errors with the same status, code and message the TypeScript global error handler produced.
//!
//! ```
//! use axum::http::StatusCode;
//! use kawalo_api::core::error::{ApiError, ErrorCode};
//!
//! let err = ApiError::not_found().with_message("Insiden tidak ditemukan.");
//! assert_eq!(err.status(), StatusCode::NOT_FOUND);
//! assert_eq!(err.code(), ErrorCode::ResourceNotFound);
//! ```

use std::fmt;

use axum::Json;
use axum::http::{HeaderValue, StatusCode, header};
use axum::response::{IntoResponse, Response};
use serde::Serialize;
use serde_json::Value;

use crate::core::config::Config;
use crate::core::response::ErrorBody;

/// Machine-readable `error_code` of the error envelope.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize)]
#[serde(rename_all = "SCREAMING_SNAKE_CASE")]
pub enum ErrorCode {
    AuthInvalidCredentials,
    AuthTokenExpired,
    AuthTokenInvalid,
    AuthTokenRevoked,
    AuthSessionStale,
    AuthReuseDetected,
    AuthUnauthorized,
    AuthForbidden,
    AuthUserNotFound,
    AuthUserInactive,
    AuthAccountLocked,
    ValidationError,
    BadRequest,
    ResourceNotFound,
    ConflictError,
    InternalServerError,
    DatabaseError,
    CacheError,
    RateLimitExceeded,
    ServiceUnavailable,
    FaceServiceUnavailable,
    FaceNotRegistered,
    FaceAlreadyRegistered,
    FaceNotMatch,
    FaceQualityLow,
    FaceDuplicate,
    AttendanceNoSchedule,
    AttendanceAlreadyCompleted,
    AttendanceOutOfRadius,
    AttendanceNonceInvalid,
    AttendanceManualUnavailable,
    ScheduleOtherShift,
}

impl ErrorCode {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::AuthInvalidCredentials => "AUTH_INVALID_CREDENTIALS",
            Self::AuthTokenExpired => "AUTH_TOKEN_EXPIRED",
            Self::AuthTokenInvalid => "AUTH_TOKEN_INVALID",
            Self::AuthTokenRevoked => "AUTH_TOKEN_REVOKED",
            Self::AuthSessionStale => "AUTH_SESSION_STALE",
            Self::AuthReuseDetected => "AUTH_REUSE_DETECTED",
            Self::AuthUnauthorized => "AUTH_UNAUTHORIZED",
            Self::AuthForbidden => "AUTH_FORBIDDEN",
            Self::AuthUserNotFound => "AUTH_USER_NOT_FOUND",
            Self::AuthUserInactive => "AUTH_USER_INACTIVE",
            Self::AuthAccountLocked => "AUTH_ACCOUNT_LOCKED",
            Self::ValidationError => "VALIDATION_ERROR",
            Self::BadRequest => "BAD_REQUEST",
            Self::ResourceNotFound => "RESOURCE_NOT_FOUND",
            Self::ConflictError => "CONFLICT_ERROR",
            Self::InternalServerError => "INTERNAL_SERVER_ERROR",
            Self::DatabaseError => "DATABASE_ERROR",
            Self::CacheError => "CACHE_ERROR",
            Self::RateLimitExceeded => "RATE_LIMIT_EXCEEDED",
            Self::ServiceUnavailable => "SERVICE_UNAVAILABLE",
            Self::FaceServiceUnavailable => "FACE_SERVICE_UNAVAILABLE",
            Self::FaceNotRegistered => "FACE_NOT_REGISTERED",
            Self::FaceAlreadyRegistered => "FACE_ALREADY_REGISTERED",
            Self::FaceNotMatch => "FACE_NOT_MATCH",
            Self::FaceQualityLow => "FACE_QUALITY_LOW",
            Self::FaceDuplicate => "FACE_DUPLICATE",
            Self::AttendanceNoSchedule => "ATTENDANCE_NO_SCHEDULE",
            Self::AttendanceAlreadyCompleted => "ATTENDANCE_ALREADY_COMPLETED",
            Self::AttendanceOutOfRadius => "ATTENDANCE_OUT_OF_RADIUS",
            Self::AttendanceNonceInvalid => "ATTENDANCE_NONCE_INVALID",
            Self::AttendanceManualUnavailable => "ATTENDANCE_MANUAL_UNAVAILABLE",
            Self::ScheduleOtherShift => "SCHEDULE_OTHER_SHIFT",
        }
    }
}

impl fmt::Display for ErrorCode {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_str())
    }
}

/// A failed request: HTTP status, error code, Indonesian message and optional details.
///
/// Constructors mirror the TypeScript `Failure` factories and carry the same default messages.
/// `details` and `Retry-After` follow `ApiResponse.fromFailure`: details are dropped in production.
#[derive(Debug, Clone)]
pub struct ApiError {
    status: StatusCode,
    code: ErrorCode,
    message: String,
    /// `details` and `data` are boxed: the error type is returned everywhere and clippy bounds its size.
    details: Option<Box<Value>>,
    data: Option<Box<Value>>,
    retry_after_seconds: Option<u64>,
}

impl ApiError {
    pub fn new(status: StatusCode, code: ErrorCode, message: impl Into<String>) -> Self {
        Self {
            status,
            code,
            message: message.into(),
            details: None,
            data: None,
            retry_after_seconds: None,
        }
    }

    /// 500 `INTERNAL_SERVER_ERROR` (`ServerFailure`).
    pub fn server() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::InternalServerError,
            "Terjadi kesalahan pada server internal",
        )
    }

    /// 500 `DATABASE_ERROR` (`DatabaseFailure`).
    pub fn database() -> Self {
        Self::new(
            StatusCode::INTERNAL_SERVER_ERROR,
            ErrorCode::DatabaseError,
            "Terjadi kesalahan saat mengakses database",
        )
    }

    /// 401 `AUTH_INVALID_CREDENTIALS` (`AuthFailure`); override status and code with
    /// [`with_status`](Self::with_status) and [`with_code`](Self::with_code).
    pub fn auth() -> Self {
        Self::new(
            StatusCode::UNAUTHORIZED,
            ErrorCode::AuthInvalidCredentials,
            "Email atau password yang Anda masukkan salah",
        )
    }

    /// 429 `AUTH_ACCOUNT_LOCKED` with `Retry-After` (`Failure.locked`).
    pub fn locked(remaining_seconds: u64) -> Self {
        let minutes = remaining_seconds.div_ceil(60);
        Self {
            retry_after_seconds: Some(remaining_seconds),
            ..Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                ErrorCode::AuthAccountLocked,
                format!(
                    "Akun Anda dikunci sementara karena beberapa kali kegagalan masuk. Silakan coba kembali dalam {minutes} menit."
                ),
            )
        }
    }

    /// 429 `RATE_LIMIT_EXCEEDED` (`RateLimitFailure`).
    pub fn rate_limit(retry_after_seconds: Option<u64>) -> Self {
        Self {
            retry_after_seconds,
            ..Self::new(
                StatusCode::TOO_MANY_REQUESTS,
                ErrorCode::RateLimitExceeded,
                "Terlalu banyak permintaan. Silakan coba beberapa saat lagi.",
            )
        }
    }

    /// 400 `VALIDATION_ERROR` (`ValidationFailure`). Note this is 400, not 422: 422 is reserved for
    /// schema failures, see [`invalid_input`](Self::invalid_input).
    pub fn validation() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ErrorCode::ValidationError,
            "Data yang dikirim tidak valid",
        )
    }

    /// 404 `RESOURCE_NOT_FOUND` (`NotFoundFailure`).
    pub fn not_found() -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            ErrorCode::ResourceNotFound,
            "Data atau sumber daya tidak ditemukan",
        )
    }

    /// 403 `AUTH_FORBIDDEN` (`ForbiddenFailure`).
    pub fn forbidden() -> Self {
        Self::new(
            StatusCode::FORBIDDEN,
            ErrorCode::AuthForbidden,
            "Anda tidak memiliki hak akses untuk tindakan ini",
        )
    }

    /// 400 `BAD_REQUEST` "Format data JSON tidak valid" (malformed JSON body).
    pub fn malformed_json() -> Self {
        Self::new(
            StatusCode::BAD_REQUEST,
            ErrorCode::BadRequest,
            "Format data JSON tidak valid",
        )
    }

    /// 422 `VALIDATION_ERROR` "Format data masukan tidak valid", the schema validation failure.
    /// `detail` is exposed outside production only.
    pub fn invalid_input(detail: impl Into<String>) -> Self {
        Self {
            details: Some(Box::new(Value::String(detail.into()))),
            ..Self::new(
                StatusCode::UNPROCESSABLE_ENTITY,
                ErrorCode::ValidationError,
                "Format data masukan tidak valid",
            )
        }
    }

    /// 404 `RESOURCE_NOT_FOUND` "Endpoint atau rute tidak ditemukan", for unknown routes.
    pub fn route_not_found() -> Self {
        Self::new(
            StatusCode::NOT_FOUND,
            ErrorCode::ResourceNotFound,
            "Endpoint atau rute tidak ditemukan",
        )
    }

    /// 500 for an unexpected failure; the cause is exposed as `details` outside production.
    pub fn internal(cause: impl fmt::Display) -> Self {
        Self::server().with_details(Value::String(cause.to_string()))
    }

    #[must_use]
    pub fn with_message(mut self, message: impl Into<String>) -> Self {
        self.message = message.into();
        self
    }

    #[must_use]
    pub fn with_status(mut self, status: StatusCode) -> Self {
        self.status = status;
        self
    }

    #[must_use]
    pub fn with_code(mut self, code: ErrorCode) -> Self {
        self.code = code;
        self
    }

    #[must_use]
    pub fn with_details(mut self, details: Value) -> Self {
        self.details = Some(Box::new(details));
        self
    }

    /// Machine-readable extras the app needs in every environment, unlike `details`.
    #[must_use]
    pub fn with_data(mut self, data: Value) -> Self {
        self.data = Some(Box::new(data));
        self
    }

    #[must_use]
    pub fn with_retry_after(mut self, seconds: u64) -> Self {
        self.retry_after_seconds = Some(seconds);
        self
    }

    pub fn status(&self) -> StatusCode {
        self.status
    }

    pub fn code(&self) -> ErrorCode {
        self.code
    }

    pub fn message(&self) -> &str {
        &self.message
    }

    pub fn details(&self) -> Option<&Value> {
        self.details.as_deref()
    }

    pub fn retry_after_seconds(&self) -> Option<u64> {
        self.retry_after_seconds
    }
}

impl fmt::Display for ApiError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{} ({}): {}",
            self.status.as_u16(),
            self.code,
            self.message
        )
    }
}

impl std::error::Error for ApiError {}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        let mut body = ErrorBody::new(
            self.message,
            self.code,
            self.details.map(|details| *details),
        );
        body.data = self.data.map(|data| *data);
        let mut response = (self.status, Json(body)).into_response();
        // Elysia handlers only set it when the value is non-zero.
        if let Some(seconds @ 1..) = self.retry_after_seconds
            && let Ok(value) = HeaderValue::from_str(&seconds.to_string())
        {
            response.headers_mut().insert(header::RETRY_AFTER, value);
        }
        response
    }
}

/// Database errors become the response of the TypeScript global handler: a generic message in
/// production, `DATABASE_ERROR` with the driver message elsewhere.
impl From<sqlx::Error> for ApiError {
    fn from(err: sqlx::Error) -> Self {
        tracing::error!(error = %err, "database error");
        if Config::global().is_production() {
            Self::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::InternalServerError,
                "Terjadi kesalahan pada sistem saat memproses data",
            )
        } else {
            Self::new(
                StatusCode::INTERNAL_SERVER_ERROR,
                ErrorCode::DatabaseError,
                "Kesalahan operasi basis data",
            )
            .with_details(Value::String(err.to_string()))
        }
    }
}

impl From<redis::RedisError> for ApiError {
    fn from(err: redis::RedisError) -> Self {
        tracing::error!(error = %err, "redis error");
        Self::internal(err)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use http_body_util::BodyExt;

    async fn body_json(response: Response) -> Value {
        let bytes = response
            .into_body()
            .collect()
            .await
            .expect("body")
            .to_bytes();
        serde_json::from_slice(&bytes).expect("json body")
    }

    #[test]
    fn factories_carry_the_typescript_defaults() {
        let cases = [
            (ApiError::server(), 500, "INTERNAL_SERVER_ERROR"),
            (ApiError::database(), 500, "DATABASE_ERROR"),
            (ApiError::auth(), 401, "AUTH_INVALID_CREDENTIALS"),
            (ApiError::locked(900), 429, "AUTH_ACCOUNT_LOCKED"),
            (ApiError::rate_limit(None), 429, "RATE_LIMIT_EXCEEDED"),
            (ApiError::validation(), 400, "VALIDATION_ERROR"),
            (ApiError::not_found(), 404, "RESOURCE_NOT_FOUND"),
            (ApiError::forbidden(), 403, "AUTH_FORBIDDEN"),
            (ApiError::malformed_json(), 400, "BAD_REQUEST"),
            (ApiError::invalid_input("x"), 422, "VALIDATION_ERROR"),
        ];
        for (err, status, code) in cases {
            assert_eq!(err.status().as_u16(), status, "{err}");
            assert_eq!(err.code().as_str(), code, "{err}");
        }
    }

    #[test]
    fn locked_rounds_minutes_up() {
        let err = ApiError::locked(61);
        assert!(
            err.message().ends_with("dalam 2 menit."),
            "{}",
            err.message()
        );
        assert_eq!(err.retry_after_seconds(), Some(61));
    }

    #[tokio::test]
    async fn response_has_envelope_and_retry_after() {
        let response = ApiError::locked(120).into_response();
        assert_eq!(response.status(), StatusCode::TOO_MANY_REQUESTS);
        assert_eq!(response.headers()[header::RETRY_AFTER], "120");
        let json = body_json(response).await;
        assert_eq!(json["success"], false);
        assert_eq!(json["error_code"], "AUTH_ACCOUNT_LOCKED");
        assert!(json.get("details").is_none());
    }

    #[tokio::test]
    async fn zero_retry_after_adds_no_header() {
        let response = ApiError::rate_limit(Some(0)).into_response();
        assert!(response.headers().get(header::RETRY_AFTER).is_none());
    }

    #[tokio::test]
    async fn details_are_serialised_outside_production() {
        let json = body_json(ApiError::invalid_input("/email: salah").into_response()).await;
        assert_eq!(json["details"], "/email: salah");
        assert_eq!(json["message"], "Format data masukan tidak valid");
    }
}
