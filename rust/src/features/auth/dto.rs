//! Request bodies and response data of the auth endpoints.

use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use crate::core::extract::{Validate, ValidationErrors};
use crate::features::auth::user::User;

/// `t.Optional(t.String())` accepts a missing key but not `null`.
fn string_not_null<'de, D: Deserializer<'de>>(deserializer: D) -> Result<Option<String>, D::Error> {
    String::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub email: String,
    pub password: String,
    #[serde(default, deserialize_with = "string_not_null")]
    pub group_uid: Option<String>,
}

impl Validate for LoginBody {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.email("email", &self.email);
        errors.length("password", &self.password, 1..);
        errors.into_result()
    }
}

#[derive(Debug, Deserialize)]
pub struct RefreshBody {
    pub refresh_token: String,
}

impl Validate for RefreshBody {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.length("refresh_token", &self.refresh_token, 1..);
        errors.into_result()
    }
}

#[derive(Debug, Deserialize)]
pub struct SwitchGroupBody {
    pub group_uid: String,
}

impl Validate for SwitchGroupBody {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.length("group_uid", &self.group_uid, 1..);
        errors.into_result()
    }
}

/// TypeBox lets any array through `t.Object` when every property is optional, so `[]` is a valid
/// logout body; other non-objects are rejected.
#[derive(Debug, Deserialize)]
#[serde(try_from = "Value")]
pub struct LogoutBody {
    pub refresh_token: Option<String>,
}

impl TryFrom<Value> for LogoutBody {
    type Error = String;

    fn try_from(value: Value) -> Result<Self, Self::Error> {
        let refresh_token = match value {
            Value::Array(_) => None,
            Value::Object(mut map) => match map.remove("refresh_token") {
                None => None,
                Some(Value::String(token)) => Some(token),
                Some(_) => return Err("refresh_token: Expected string".to_owned()),
            },
            _ => return Err("Expected object".to_owned()),
        };
        Ok(Self { refresh_token })
    }
}

impl Validate for LogoutBody {}

/// The access/refresh token pair returned to the client.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Tokens {
    pub access_token: String,
    pub refresh_token: String,
    pub token_type: String,
    pub expires_in: u64,
}

/// Data of a successful login or group switch.
#[derive(Debug, Serialize)]
pub struct LoginData {
    pub user: User,
    pub tokens: Tokens,
}
