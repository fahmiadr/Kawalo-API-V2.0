//! Authentication extractors, ported from `auth.middleware.ts`.
//!
//! [`CurrentUser`] is the equivalent of the `requireAuth` macro; [`AuthContext`] is the
//! equivalent of the derived `currentUser` / `isSessionStale` pair for routes that need the
//! `requireRole` flavour of the 401 message.

use axum::extract::{FromRequest, FromRequestParts, Request};
use axum::http::request::Parts;
use axum::http::{HeaderMap, StatusCode, header};

use crate::core::constants::AUTH_TOKEN_TYPE;
use crate::core::error::{ApiError, ErrorCode};
use crate::core::extract::json::ValidatedJson;
use crate::core::extract::schema::Body;
use crate::core::security::verify_access_token;
use crate::core::state::AppState;

const MSG_UNAUTHORIZED: &str = "Akses tidak diizinkan. Token tidak valid atau telah kedaluwarsa.";
const MSG_UNAUTHORIZED_FOR_ROLE: &str = "Akses tidak diizinkan. Silakan login terlebih dahulu.";
const MSG_SESSION_STALE: &str = "Sesi telah diperbarui di sistem. Silakan perbarui sesi Anda.";
const MSG_FORBIDDEN: &str = "Anda tidak memiliki wewenang untuk mengakses sumber daya ini.";
const MSG_NO_GROUP: &str = "Pengguna belum memilih grup aktif.";

/// A verified caller (`AuthenticatedUserPayload`): valid token and current session version.
///
/// Rejects with 401 `AUTH_UNAUTHORIZED` or `AUTH_SESSION_STALE`. For routes that also needed
/// `requireRole` in TypeScript, call [`require_role`](Self::require_role) first thing; its 401
/// text matches `requireAuth`, which is what the TypeScript routes that declared both returned.
///
/// ```ignore
/// async fn handler(user: CurrentUser) -> ApiResult<Out> {
///     user.require_role(&["ADMIN", "GUARD"])?;
///     let group = user.require_group_uid()?;
///     ..
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CurrentUser {
    pub sub: String,
    pub email: String,
    pub name: Option<String>,
    /// Upper-cased category in the active group (`ADMIN`, `GUARD`, `USER`).
    pub role: Option<String>,
    pub group_uid: Option<String>,
    pub allow_admin: bool,
    pub allow_patrol: bool,
    pub allow_mon_patrol: bool,
    pub session_ver: i64,
}

impl CurrentUser {
    /// Passes when the role (case-insensitive) is one of `roles`.
    ///
    /// # Errors
    /// 403 `AUTH_FORBIDDEN` otherwise, including when the token has no role.
    pub fn require_role(&self, roles: &[&str]) -> Result<(), ApiError> {
        let allowed = self
            .role
            .as_deref()
            .is_some_and(|role| roles.iter().any(|r| r.trim().eq_ignore_ascii_case(role)));
        if allowed { Ok(()) } else { Err(forbidden()) }
    }

    /// The active group from the token. Tenant data must always be filtered by it.
    ///
    /// # Errors
    /// 400 `VALIDATION_ERROR` "Pengguna belum memilih grup aktif." when the token has none.
    pub fn require_group_uid(&self) -> Result<&str, ApiError> {
        self.group_uid
            .as_deref()
            .filter(|g| !g.is_empty())
            .ok_or_else(|| ApiError::validation().with_message(MSG_NO_GROUP))
    }
}

/// Outcome of looking at the `Authorization` header (the TypeScript `derive` step).
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Auth {
    Anonymous,
    /// Valid token, but its session version is behind the one in Redis.
    Stale,
    User(CurrentUser),
}

impl Auth {
    /// `requireAuth`.
    ///
    /// # Errors
    /// 401 `AUTH_SESSION_STALE` or `AUTH_UNAUTHORIZED`.
    pub fn require_auth(self) -> Result<CurrentUser, ApiError> {
        match self {
            Self::User(user) => Ok(user),
            Self::Stale => Err(stale()),
            Self::Anonymous => Err(unauthorized(MSG_UNAUTHORIZED)),
        }
    }

    /// `requireRole`, whose anonymous 401 text differs from `requireAuth`.
    ///
    /// # Errors
    /// 401 as above, or 403 `AUTH_FORBIDDEN` when the role is not allowed.
    pub fn require_role(self, roles: &[&str]) -> Result<CurrentUser, ApiError> {
        let user = match self {
            Self::User(user) => user,
            Self::Stale => return Err(stale()),
            Self::Anonymous => return Err(unauthorized(MSG_UNAUTHORIZED_FOR_ROLE)),
        };
        user.require_role(roles)?;
        Ok(user)
    }
}

/// Extractor form of [`Auth`] that never rejects, except on Redis errors (500).
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthContext(pub Auth);

fn unauthorized(message: &str) -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        ErrorCode::AuthUnauthorized,
        message,
    )
}

fn stale() -> ApiError {
    ApiError::new(
        StatusCode::UNAUTHORIZED,
        ErrorCode::AuthSessionStale,
        MSG_SESSION_STALE,
    )
}

fn forbidden() -> ApiError {
    ApiError::new(
        StatusCode::FORBIDDEN,
        ErrorCode::AuthForbidden,
        MSG_FORBIDDEN,
    )
}

/// Verifies a raw access token and its session version; usable outside HTTP headers, for example
/// for a WebSocket token passed in the query string.
///
/// # Errors
/// Redis failures while checking the session version.
pub async fn authenticate_token(state: &AppState, token: &str) -> Result<Auth, ApiError> {
    let Some(payload) = verify_access_token(&state.keys, token) else {
        return Ok(Auth::Anonymous);
    };
    let (Some(sub), Some(email)) = (payload.sub, payload.email) else {
        return Ok(Auth::Anonymous);
    };
    if !state
        .sessions
        .validate_or_rehydrate(&sub, payload.session_ver)
        .await?
    {
        return Ok(Auth::Stale);
    }
    Ok(Auth::User(CurrentUser {
        sub,
        email,
        name: payload.name,
        role: payload.role,
        group_uid: payload.group_uid,
        allow_admin: payload.allow_admin,
        allow_patrol: payload.allow_patrol,
        allow_mon_patrol: payload.allow_mon_patrol,
        session_ver: payload.session_ver,
    }))
}

/// `Bearer <token>` from the headers; `None` for a missing prefix or empty token.
fn bearer_token(headers: &HeaderMap) -> Option<&str> {
    let value = headers.get(header::AUTHORIZATION)?.to_str().ok()?;
    let token = value
        .strip_prefix(AUTH_TOKEN_TYPE)?
        .strip_prefix(' ')?
        .trim();
    (!token.is_empty()).then_some(token)
}

async fn authenticate(headers: &HeaderMap, state: &AppState) -> Result<Auth, ApiError> {
    match bearer_token(headers) {
        Some(token) => authenticate_token(state, token).await,
        None => Ok(Auth::Anonymous),
    }
}

impl FromRequestParts<AppState> for AuthContext {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        authenticate(&parts.headers, state).await.map(Self)
    }
}

impl FromRequestParts<AppState> for CurrentUser {
    type Rejection = ApiError;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        authenticate(&parts.headers, state).await?.require_auth()
    }
}

/// JSON body plus caller for routes with a request schema.
///
/// Elysia validates the body before running `requireAuth`/`requireRole`, so a bad body with no
/// token is 422, not 401. axum extractors run in argument order and the body must come last, which
/// would flip that order; this extractor parses and validates the body first, then resolves the
/// caller. The auth outcome is returned unresolved so the handler picks `require_auth` or
/// `require_role` and gets the matching 401 text.
///
/// ```ignore
/// async fn create(AuthedJson(auth, body): AuthedJson<CreateDto>) -> ApiResult<Out> {
///     let user = auth.require_role(&["ADMIN", "GUARD"])?;
///     ..
/// }
/// ```
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthedJson<T>(pub Auth, pub T);

impl<T> FromRequest<AppState> for AuthedJson<T>
where
    T: Body + Send,
{
    type Rejection = ApiError;

    async fn from_request(req: Request, state: &AppState) -> Result<Self, Self::Rejection> {
        let headers = req.headers().clone();
        let ValidatedJson(body) = ValidatedJson::<T>::from_request(req, state).await?;
        let auth = authenticate(&headers, state).await?;
        Ok(Self(auth, body))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn user(role: Option<&str>) -> CurrentUser {
        CurrentUser {
            sub: "u".into(),
            email: "u@x.id".into(),
            name: None,
            role: role.map(str::to_owned),
            group_uid: Some("g1".into()),
            allow_admin: false,
            allow_patrol: false,
            allow_mon_patrol: false,
            session_ver: 1,
        }
    }

    #[test]
    fn bearer_parsing_requires_prefix_and_token() {
        let mut headers = HeaderMap::new();
        assert_eq!(bearer_token(&headers), None);
        for (value, expected) in [
            ("Bearer abc", Some("abc")),
            ("Bearer   abc  ", Some("abc")),
            ("Bearer ", None),
            ("Bearer", None),
            ("bearer abc", None),
            ("Basic abc", None),
        ] {
            headers.insert(header::AUTHORIZATION, value.parse().expect("value"));
            assert_eq!(bearer_token(&headers), expected, "{value}");
        }
    }

    #[test]
    fn require_role_is_case_insensitive_and_denies_missing_roles() {
        assert!(
            user(Some("GUARD"))
                .require_role(&["admin", " Guard "])
                .is_ok()
        );
        for role in [Some("USER"), None] {
            let err = user(role)
                .require_role(&["ADMIN", "GUARD"])
                .expect_err("denied");
            assert_eq!(err.status(), StatusCode::FORBIDDEN);
            assert_eq!(err.code(), ErrorCode::AuthForbidden);
            assert_eq!(err.message(), MSG_FORBIDDEN);
        }
    }

    #[test]
    fn rejection_messages_match_the_typescript_macros() {
        let anonymous = Auth::Anonymous.require_auth().expect_err("401");
        assert_eq!(anonymous.message(), MSG_UNAUTHORIZED);
        assert_eq!(anonymous.code(), ErrorCode::AuthUnauthorized);

        let for_role = Auth::Anonymous.require_role(&["ADMIN"]).expect_err("401");
        assert_eq!(for_role.message(), MSG_UNAUTHORIZED_FOR_ROLE);

        for result in [
            Auth::Stale.require_auth(),
            Auth::Stale.require_role(&["ADMIN"]),
        ] {
            let err = result.expect_err("401");
            assert_eq!(
                (err.status(), err.code()),
                (StatusCode::UNAUTHORIZED, ErrorCode::AuthSessionStale)
            );
            assert_eq!(err.message(), MSG_SESSION_STALE);
        }
    }

    #[test]
    fn group_uid_is_required_for_tenant_queries() {
        assert_eq!(user(None).require_group_uid().expect("group"), "g1");
        let none = CurrentUser {
            group_uid: None,
            ..user(None)
        };
        let err = none.require_group_uid().expect_err("no group");
        assert_eq!(err.status(), StatusCode::BAD_REQUEST);
        assert_eq!(err.message(), MSG_NO_GROUP);
    }
}
