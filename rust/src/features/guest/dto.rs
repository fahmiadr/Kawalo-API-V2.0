//! Request validation (TypeBox DTOs) and response shapes.

use std::collections::HashMap;

use axum::extract::{FromRequestParts, Path};
use axum::http::request::Parts;
use serde::{Deserialize, Deserializer, Serialize};

use crate::core::clean::Patch;
use crate::core::constants::MAX_PAGE_SIZE;
use crate::core::error::ApiError;
use crate::core::extract::{Validate, ValidationErrors};

const IDENTITY_TYPES: [&str; 4] = ["KTP", "SIM", "PASPOR", "LAINNYA"];
const ID_PATTERN: &str = "^[a-zA-Z0-9_-]+$";
const NUMERIC_PATTERN: &str = "^[0-9]+$";

/// A field that may be omitted but not `null` (TypeBox `t.Optional(t.String())`); `Option`
/// alone would accept `null`.
fn not_null<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

fn is_id(value: &str) -> bool {
    !value.is_empty()
        && value
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
}

fn is_numeric(value: &str) -> bool {
    !value.is_empty() && value.bytes().all(|b| b.is_ascii_digit())
}

fn set_value(patch: &Patch<String>) -> Option<&str> {
    match patch {
        Patch::Set(value) => Some(value),
        Patch::Missing | Patch::Clear => None,
    }
}

/// `:id` path parameter (numeric `GuestId` or `QRUID`). Elysia validates params before the auth
/// macros run, so this must precede the auth extractor in handler arguments.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct GuestPath(pub String);

impl<S: Send + Sync> FromRequestParts<S> for GuestPath {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(id) = Path::<String>::from_request_parts(parts, state)
            .await
            .map_err(ApiError::internal)?;
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

/// Query string of `GET /guests`. Parsed by hand because Elysia's `t.Numeric` and repeated
/// parameters behave unlike serde: the last occurrence wins, and numbers are read with
/// JavaScript's `Number()` (`0x5`, `1e1`, `" 2 "`).
#[derive(Debug)]
pub struct ListQuery {
    pub search: Option<String>,
    pub limit: Option<f64>,
    pub offset: Option<f64>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub created_by: Option<String>,
    pub is_active: Option<String>,
}

/// `Number(text)` for non-empty `text`; `None` for `NaN`.
fn js_number(text: &str) -> Option<f64> {
    let text = text.trim();
    if text.is_empty() {
        return Some(0.0);
    }
    let radix_digits = |prefix: [&str; 2], radix: u32| {
        let digits = prefix.iter().find_map(|p| text.strip_prefix(p))?;
        // `from_str_radix` would also take a leading sign, which `Number()` does not.
        if !digits.bytes().all(|b| b.is_ascii_alphanumeric()) {
            return None;
        }
        u64::from_str_radix(digits, radix).ok().map(|n| n as f64)
    };
    if let Some(n) = radix_digits(["0x", "0X"], 16)
        .or_else(|| radix_digits(["0b", "0B"], 2))
        .or_else(|| radix_digits(["0o", "0O"], 8))
    {
        return Some(n);
    }
    // Rust also accepts `inf` and `nan`; JavaScript only spells `Infinity`.
    if text == "Infinity" || text == "+Infinity" {
        return Some(f64::INFINITY);
    }
    if text == "-Infinity" {
        return Some(f64::NEG_INFINITY);
    }
    if text
        .bytes()
        .any(|b| b.is_ascii_alphabetic() && !matches!(b, b'e' | b'E'))
    {
        return None;
    }
    text.parse::<f64>().ok()
}

impl<S: Send + Sync> FromRequestParts<S> for ListQuery {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, _state: &S) -> Result<Self, Self::Rejection> {
        let mut last: HashMap<String, String> = HashMap::new();
        for (key, value) in form_urlencoded::parse(parts.uri.query().unwrap_or("").as_bytes()) {
            last.insert(key.into_owned(), value.into_owned());
        }
        let mut numeric = |key: &str| -> Result<Option<f64>, ApiError> {
            let Some(raw) = last.remove(key) else {
                return Ok(None);
            };
            // Elysia rejects the empty string even though `Number("")` is 0.
            let number = if raw.is_empty() {
                None
            } else {
                js_number(&raw)
            };
            number
                .map(Some)
                .ok_or_else(|| ApiError::invalid_input(format!("/{key}: Expected union value")))
        };
        let limit = numeric("limit")?;
        let offset = numeric("offset")?;
        let query = Self {
            search: last.remove("search"),
            limit,
            offset,
            start_date: last.remove("start_date"),
            end_date: last.remove("end_date"),
            created_by: last.remove("created_by"),
            is_active: last.remove("is_active"),
        };
        query
            .validate()
            .map_err(|errors| ApiError::invalid_input(errors.to_string()))?;
        Ok(query)
    }
}

impl Validate for ListQuery {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.opt_length("search", self.search.as_deref(), 1..);
        errors.opt_range("limit", self.limit, 0.0..=f64::from(MAX_PAGE_SIZE));
        errors.opt_range("offset", self.offset, 0.0..);
        errors.opt_length("created_by", self.created_by.as_deref(), 1..);
        if self
            .is_active
            .as_deref()
            .is_some_and(|v| !matches!(v, "true" | "false"))
        {
            errors.push("is_active", "Expected union value");
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

impl Validate for CreateGuestBody {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.length("guest_name", &self.guest_name, 1..=128);
        errors.length("guest_destination", &self.guest_destination, 1..=128);
        errors.opt_length("guest_purpose", self.guest_purpose.as_deref(), ..=128);
        errors.opt_length("guest_number", self.guest_number.as_deref(), ..=32);
        errors.opt_length("guest_identity", self.guest_identity.as_deref(), ..=64);
        if let Some(kind) = &self.guest_identity_type {
            errors.one_of("guest_identity_type", kind, &IDENTITY_TYPES);
        }
        errors.length("guest_type", &self.guest_type, 1..=64);
        errors.opt_length("guest_address", self.guest_address.as_deref(), ..=512);
        errors.opt_length(
            "guest_to_user_email",
            self.guest_to_user_email.as_deref(),
            ..=128,
        );
        errors.opt_email("guest_to_user_email", self.guest_to_user_email.as_deref());
        errors.opt_length("guest_photo", self.guest_photo.as_deref(), ..=2048);
        errors.into_result()
    }
}

/// Body of PUT and PATCH. Fields required on create cannot be cleared, so they are plain
/// `Option`s; the rest are tri-state.
#[derive(Debug, Deserialize)]
pub struct UpdateGuestBody {
    #[serde(default, deserialize_with = "not_null")]
    pub guest_name: Option<String>,
    #[serde(default, deserialize_with = "not_null")]
    pub guest_destination: Option<String>,
    #[serde(default)]
    pub guest_purpose: Patch<String>,
    #[serde(default)]
    pub guest_number: Patch<String>,
    #[serde(default)]
    pub guest_identity: Patch<String>,
    #[serde(default)]
    pub guest_identity_type: Patch<String>,
    #[serde(default, deserialize_with = "not_null")]
    pub guest_type: Option<String>,
    #[serde(default)]
    pub guest_address: Patch<String>,
    #[serde(default)]
    pub guest_to_user_email: Patch<String>,
    #[serde(default)]
    pub guest_photo: Patch<String>,
}

impl Validate for UpdateGuestBody {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.opt_length("guest_name", self.guest_name.as_deref(), 1..=128);
        errors.opt_length(
            "guest_destination",
            self.guest_destination.as_deref(),
            1..=128,
        );
        errors.opt_length("guest_purpose", set_value(&self.guest_purpose), ..=128);
        errors.opt_length("guest_number", set_value(&self.guest_number), ..=32);
        errors.opt_length("guest_identity", set_value(&self.guest_identity), ..=64);
        if let Some(kind) = set_value(&self.guest_identity_type) {
            errors.one_of("guest_identity_type", kind, &IDENTITY_TYPES);
        }
        errors.opt_length("guest_type", self.guest_type.as_deref(), 1..=64);
        errors.opt_length("guest_address", set_value(&self.guest_address), ..=512);
        let email = set_value(&self.guest_to_user_email);
        errors.opt_length("guest_to_user_email", email, ..=128);
        errors.opt_email("guest_to_user_email", email);
        errors.opt_length("guest_photo", set_value(&self.guest_photo), ..=2048);
        errors.into_result()
    }
}

/// Body of acknowledge and checkout: an optional QR code that takes precedence over the path id.
#[derive(Debug, Deserialize)]
pub struct QrBody {
    pub qr_uid: Option<String>,
}

impl Validate for QrBody {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.opt_length("qr_uid", self.qr_uid.as_deref(), ..=64);
        errors.into_result()
    }
}

#[derive(Debug, Deserialize)]
pub struct DeleteBatchBody {
    pub ids: Vec<String>,
}

impl Validate for DeleteBatchBody {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if self.ids.is_empty() {
            errors.push("ids", "Expected array length to be greater or equal to 1");
        }
        for (index, id) in self.ids.iter().enumerate() {
            if !is_numeric(id) {
                errors.push(
                    &format!("ids/{index}"),
                    format!("Expected string to match '{NUMERIC_PATTERN}'"),
                );
            }
        }
        errors.into_result()
    }
}

#[derive(Debug, Serialize)]
pub struct DeleteBatchResult {
    pub deleted_count: u64,
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
    fn required_update_fields_reject_null() {
        for field in ["guest_name", "guest_destination", "guest_type"] {
            assert!(update(json!({ field: null })).is_err(), "{field}");
        }
    }

    #[test]
    fn update_body_enforces_limits_and_formats() {
        let body = update(json!({
            "guest_name": "",
            "guest_number": "x".repeat(33),
            "guest_identity_type": "BPJS",
            "guest_to_user_email": "bukan-email",
        }))
        .expect("body");
        assert_eq!(
            body.validate().expect_err("invalid").to_string(),
            "/guest_name: Expected string length greater or equal to 1, \
             /guest_number: Expected string length less or equal to 32, \
             /guest_identity_type: Expected one of: KTP, SIM, PASPOR, LAINNYA, \
             /guest_to_user_email: Property 'guest_to_user_email' should be email"
        );
    }

    #[test]
    fn id_and_numeric_patterns() {
        assert!(is_id("a1_-B") && is_id("123"));
        assert!(!is_id("") && !is_id("a b") && !is_id("a/b") && !is_id("a\n"));
        assert!(is_numeric("007") && !is_numeric("1a") && !is_numeric(""));
    }

    #[test]
    fn batch_body_requires_numeric_ids() {
        let body = DeleteBatchBody {
            ids: vec!["1".into(), "x".into()],
        };
        assert_eq!(
            body.validate().expect_err("invalid").to_string(),
            "/ids/1: Expected string to match '^[0-9]+$'"
        );
        let empty = DeleteBatchBody { ids: vec![] };
        assert!(empty.validate().is_err());
    }

    #[test]
    fn js_number_matches_number_of_strings() {
        for (text, expected) in [
            ("5", 5.0),
            (" 2 ", 2.0),
            ("   ", 0.0),
            ("0x5", 5.0),
            ("0XfF", 255.0),
            ("0b11", 3.0),
            ("0o17", 15.0),
            ("1e1", 10.0),
            (".5", 0.5),
            ("5.", 5.0),
            ("+5", 5.0),
            ("-0", 0.0),
        ] {
            assert_eq!(js_number(text), Some(expected), "{text:?}");
        }
        for text in [
            "abc", "1_0", "0x", "0x+5", "0b12", "inf", "nan", "1e", "--1",
        ] {
            assert_eq!(js_number(text), None, "{text:?}");
        }
        assert_eq!(js_number("Infinity"), Some(f64::INFINITY));
    }

    #[test]
    fn list_query_accepts_only_true_or_false() {
        let query = |is_active: &str| ListQuery {
            search: None,
            limit: Some(0.0),
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
