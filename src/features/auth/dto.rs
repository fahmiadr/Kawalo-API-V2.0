//! Request bodies and response data of the auth endpoints.

use serde::{Deserialize, Serialize};

use crate::core::extract::schema::{Body, Field, Kind, NO_LIMIT, Schema};
use crate::features::auth::user::User;

#[derive(Debug, Deserialize)]
pub struct LoginBody {
    pub email: String,
    pub password: String,
    pub group_uid: Option<String>,
}

impl Body for LoginBody {
    const SCHEMA: Schema = Schema::new(&[
        Field::required("email", Kind::email(NO_LIMIT)),
        Field::required("password", Kind::text(1, NO_LIMIT)),
        Field::optional("group_uid", Kind::text(0, NO_LIMIT)),
    ]);
}

#[derive(Debug, Deserialize)]
pub struct RefreshBody {
    pub refresh_token: String,
}

impl Body for RefreshBody {
    const SCHEMA: Schema =
        Schema::new(&[Field::required("refresh_token", Kind::text(1, NO_LIMIT))]);
}

#[derive(Debug, Deserialize)]
pub struct SwitchGroupBody {
    pub group_uid: String,
}

impl Body for SwitchGroupBody {
    const SCHEMA: Schema = Schema::new(&[Field::required("group_uid", Kind::text(1, NO_LIMIT))]);
}

#[derive(Debug, Deserialize)]
pub struct LogoutBody {
    pub refresh_token: Option<String>,
}

impl Body for LogoutBody {
    const SCHEMA: Schema =
        Schema::new(&[Field::optional("refresh_token", Kind::text(0, NO_LIMIT))]);
}

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

#[derive(Debug, Deserialize)]
pub struct AvatarBody {
    pub avatar_url: Option<String>,
}

impl Body for AvatarBody {
    const SCHEMA: Schema = Schema::new(&[Field::required_nullable(
        "avatar_url",
        Kind::text(0, NO_LIMIT),
    )]);
}

#[derive(Debug, Serialize)]
pub struct AvatarData {
    pub avatar_url: Option<String>,
}

/// Bounded only against abuse; the real rules on the trimmed value live in the handler.
#[derive(Deserialize)]
pub struct ChangePasswordBody {
    pub current_password: String,
    pub new_password: String,
}

impl Body for ChangePasswordBody {
    const SCHEMA: Schema = Schema::new(&[
        Field::required("current_password", Kind::text(1, 1024)),
        Field::required("new_password", Kind::text(1, 1024)),
    ]);
}
