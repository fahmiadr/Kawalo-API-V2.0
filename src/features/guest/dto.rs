//! Request validation (TypeBox DTOs) and response shapes.

use axum::extract::rejection::PathRejection;
use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use serde::{Deserialize, Serialize};

use crate::core::clean::Patch;
use crate::core::constants::MAX_PAGE_SIZE;
use crate::core::error::ApiError;
use crate::core::extract::schema::{Body, Field, Kind, NO_LIMIT, Schema};
use crate::core::extract::{Numeric, Validate, ValidationErrors};

const IDENTITY_TYPES: [&str; 4] = ["KTP", "SIM", "PASPOR", "LAINNYA"];
const ID_PATTERN: &str = "^[a-zA-Z0-9_-]+$";
/// Path segment of the static master-types route.
const MASTER_TYPES_ID: &str = "master-types";

fn is_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

/// `:id` path parameter (numeric `GuestId` or `QRUID`). Elysia validates params before the auth
/// macros run, so this must precede the auth extractor in handler arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestPath(pub String);

impl<S: Send + Sync> FromRequestParts<S> for GuestPath {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let id = match Path::<String>::from_request_parts(parts, state).await {
            Ok(Path(id)) => id,
            // `/guests/master-types` is a static route without `:id`; Elysia lets the `/:id`
            // handlers answer its other methods with id `master-types`.
            Err(
                PathRejection::MissingPathParams(_)
                | PathRejection::FailedToDeserializePathParams(_),
            ) => MASTER_TYPES_ID.to_owned(),
            Err(err) => return Err(ApiError::internal(err)),
        };
        let mut errors = ValidationErrors::new();
        errors.length("id", &id, 1..=64);
        if !is_id(&id) {
            errors.push("id", format!("Expected string to match '{ID_PATTERN}'"));
        }
        errors
            .into_result()
            .map_err(|errors| ApiError::invalid_input(errors.to_string()))?;
        Ok(Self(id))
    }
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub search: Option<String>,
    pub limit: Option<Numeric>,
    pub offset: Option<Numeric>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub created_by: Option<String>,
    pub is_active: Option<String>,
}

impl Validate for ListQuery {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.opt_length("search", self.search.as_deref(), 1..);
        errors.numeric("limit", self.limit, 0.0..=f64::from(MAX_PAGE_SIZE));
        errors.numeric("offset", self.offset, 0.0..);
        errors.opt_length("created_by", self.created_by.as_deref(), 1..);
        if self
            .is_active
            .as_deref()
            .is_some_and(|v| !matches!(v, "true" | "false"))
        {
            errors.push(
                "is_active",
                "Property 'is_active' should be one of: 'boolean', 'undefined'",
            );
        }
        errors.into_result()
    }
}

#[derive(Debug, Deserialize)]
pub struct CreateGuestBody {
    pub guest_name: String,
    pub guest_destination: String,
    pub guest_purpose: Option<String>,
    pub guest_number: Option<String>,
    pub guest_identity: Option<String>,
    pub guest_identity_type: Option<String>,
    pub guest_type: String,
    pub guest_address: Option<String>,
    pub guest_to_user_email: Option<String>,
    pub guest_photo: Option<String>,
    pub guest_dt: Option<String>,
}

impl Body for CreateGuestBody {
    const SCHEMA: Schema = Schema::new(&[
        Field::required("guest_name", Kind::text(1, 128)),
        Field::required("guest_destination", Kind::text(1, 128)),
        Field::nullable("guest_purpose", Kind::text(0, 128)),
        Field::nullable("guest_number", Kind::text(0, 32)),
        Field::nullable("guest_identity", Kind::text(0, 64)),
        Field::nullable("guest_identity_type", Kind::OneOf(&IDENTITY_TYPES)),
        Field::required("guest_type", Kind::text(1, 64)),
        Field::nullable("guest_address", Kind::text(0, 512)),
        Field::nullable("guest_to_user_email", Kind::email(128)),
        Field::nullable("guest_photo", Kind::text(0, 2048)),
        Field::nullable("guest_dt", Kind::text(0, NO_LIMIT)),
    ]);
}

/// Body of PUT and PATCH. Fields required on create cannot be cleared, so they are plain
/// `Option`s; the rest are tri-state.
#[derive(Debug, Deserialize)]
pub struct UpdateGuestBody {
    pub guest_name: Option<String>,
    pub guest_destination: Option<String>,
    #[serde(default)]
    pub guest_purpose: Patch<String>,
    #[serde(default)]
    pub guest_number: Patch<String>,
    #[serde(default)]
    pub guest_identity: Patch<String>,
    #[serde(default)]
    pub guest_identity_type: Patch<String>,
    pub guest_type: Option<String>,
    #[serde(default)]
    pub guest_address: Patch<String>,
    #[serde(default)]
    pub guest_to_user_email: Patch<String>,
    #[serde(default)]
    pub guest_photo: Patch<String>,
}

impl Body for UpdateGuestBody {
    const SCHEMA: Schema = Schema::new(&[
        Field::optional("guest_name", Kind::text(1, 128)),
        Field::optional("guest_destination", Kind::text(1, 128)),
        Field::nullable("guest_purpose", Kind::text(0, 128)),
        Field::nullable("guest_number", Kind::text(0, 32)),
        Field::nullable("guest_identity", Kind::text(0, 64)),
        Field::nullable("guest_identity_type", Kind::OneOf(&IDENTITY_TYPES)),
        Field::optional("guest_type", Kind::text(1, 64)),
        Field::nullable("guest_address", Kind::text(0, 512)),
        Field::nullable("guest_to_user_email", Kind::email(128)),
        Field::nullable("guest_photo", Kind::text(0, 2048)),
    ]);
}

/// Body of acknowledge and checkout: an optional QR code that takes precedence over the path id.
#[derive(Debug, Deserialize)]
pub struct QrBody {
    pub qr_uid: Option<String>,
}

impl Body for QrBody {
    const SCHEMA: Schema = Schema::new(&[Field::nullable("qr_uid", Kind::text(0, 64))]);
}

#[derive(Debug, Serialize)]
pub struct MasterGuestType {
    pub id: String,
    pub uid: String,
    pub name: String,
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

    fn update(body: serde_json::Value) -> Result<UpdateGuestBody, serde_json::Error> {
        serde_json::from_value(body)
    }

    #[test]
    fn update_body_distinguishes_missing_null_and_value() {
        let body = update(json!({"guest_purpose": null, "guest_number": "B 1"})).expect("body");
        assert_eq!(body.guest_purpose, Patch::Clear);
        assert_eq!(body.guest_number, Patch::Set("B 1".to_owned()));
        assert_eq!(body.guest_address, Patch::Missing);
        assert_eq!(body.guest_name, None);
    }

    #[test]
    fn id_pattern() {
        assert!(is_id("a1_-B") && is_id("123"));
        assert!(!is_id("") && !is_id("a b") && !is_id("a/b") && !is_id("a\n"));
    }

    #[test]
    fn list_query_accepts_only_true_or_false() {
        let query = |is_active: &str| ListQuery {
            search: None,
            limit: Some(Numeric::Value(0.0)),
            offset: None,
            start_date: None,
            end_date: None,
            created_by: None,
            is_active: Some(is_active.to_owned()),
        };
        assert!(query("true").validate().is_ok() && query("false").validate().is_ok());
        assert!(query("yes").validate().is_err());
    }
}
