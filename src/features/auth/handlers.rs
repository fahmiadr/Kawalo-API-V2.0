//! Login, refresh, group switch and logout.

use std::time::{SystemTime, UNIX_EPOCH};

use axum::extract::State;
use axum::http::StatusCode;

use crate::core::clean::clean_string;
use crate::core::client_ip::ClientIp;
use crate::core::constants::AUTH_TOKEN_TYPE;
use crate::core::error::{ApiError, ErrorCode};
use crate::core::extract::{Auth, AuthedJson, ValidatedJson};
use crate::core::response::{ApiResult, ok};
use crate::core::security::{
    JwtUserPayload, generate_refresh_token, hash_token, sign_access_token_blocking,
};
use crate::core::state::AppState;
use crate::features::auth::dto::{
    LoginBody, LoginData, LogoutBody, RefreshBody, SwitchGroupBody, Tokens,
};
use crate::features::auth::login_limiter::LoginLimiter;
use crate::features::auth::password_check::verify_login;
use crate::features::auth::sessions::{RefreshSession, Sessions, now_unix};
use crate::features::auth::user::{self, GroupAccess, User};

const MSG_NOT_MEMBER: &str = "Anda tidak terdaftar sebagai anggota pada grup yang dipilih";
const MSG_INACTIVE: &str =
    "Akun Anda belum aktif atau belum disetujui. Silakan hubungi administrator Anda.";
const MSG_USER_NOT_FOUND: &str = "Pengguna tidak ditemukan";
const LOCK_SECONDS: u64 = 5;

fn forbidden(message: &str, code: ErrorCode) -> ApiError {
    ApiError::auth()
        .with_status(StatusCode::FORBIDDEN)
        .with_code(code)
        .with_message(message)
}

fn unauthorized(message: &str, code: ErrorCode) -> ApiError {
    ApiError::auth().with_code(code).with_message(message)
}

fn user_not_found() -> ApiError {
    unauthorized(MSG_USER_NOT_FOUND, ErrorCode::AuthUserNotFound)
}

fn inactive() -> ApiError {
    forbidden(MSG_INACTIVE, ErrorCode::AuthUserInactive)
}

/// Signs an access token for `group` and draws a new refresh token; returns the pair and the
/// hash under which the refresh token is stored.
async fn mint_tokens(
    state: &AppState,
    user: &User,
    group: &GroupAccess,
) -> Result<(Tokens, String), ApiError> {
    let session_ver = state.sessions.get_or_init_version(&user.id).await?;
    let payload = JwtUserPayload {
        sub: Some(user.id.clone()),
        email: Some(user.email.clone()),
        name: Some(user.name.clone()),
        role: group.category.clone(),
        group_uid: Some(group.group_uid.clone()),
        allow_admin: group.allow_admin,
        allow_patrol: group.allow_patrol,
        allow_mon_patrol: group.allow_mon_patrol,
        session_ver,
    };
    let signed =
        sign_access_token_blocking(&state.keys, state.config.jwt_expires_in, payload).await?;
    let refresh_token = generate_refresh_token();
    let hash = hash_token(&refresh_token);
    let tokens = Tokens {
        access_token: signed.token,
        refresh_token,
        token_type: AUTH_TOKEN_TYPE.to_owned(),
        expires_in: signed.expires_in,
    };
    Ok((tokens, hash))
}

fn new_session(
    state: &AppState,
    user_id: &str,
    token_hash: String,
    family_id: String,
    group_uid: &str,
) -> RefreshSession {
    let expires_at = state
        .config
        .auth
        .refresh_token_expiry(SystemTime::now())
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| i64::try_from(d.as_secs()).unwrap_or(i64::MAX));
    RefreshSession {
        user_id: user_id.to_owned(),
        token_hash,
        family_id,
        active_group_uid: Some(group_uid.to_owned()),
        created_at: now_unix(),
        expires_at,
    }
}

/// Starts a new token family for `user` in `group` and stores its refresh session.
async fn start_session(
    state: &AppState,
    user: &User,
    group: &GroupAccess,
) -> Result<Tokens, ApiError> {
    let (tokens, hash) = mint_tokens(state, user, group).await?;
    let family_id = uuid::Uuid::new_v4().to_string();
    let session = new_session(state, &user.id, hash, family_id, &group.group_uid);
    Sessions::new(state.redis.clone()).save(&session).await?;
    Ok(tokens)
}

pub async fn login(
    State(state): State<AppState>,
    ClientIp(ip): ClientIp,
    ValidatedJson(body): ValidatedJson<LoginBody>,
) -> ApiResult<LoginData> {
    let credentials = clean_string(Some(&body.email)).zip(clean_string(Some(&body.password)));
    let Some((email, password)) = credentials else {
        return Err(ApiError::auth());
    };
    let group_uid = clean_string(body.group_uid.as_deref());
    let ip = Some(ip.as_str()).filter(|ip| !ip.is_empty());
    let limiter = LoginLimiter::new(state.redis.clone(), &state.config.auth);

    if let Some(ip) = ip
        && let Some(retry_after) = limiter.ip_blocked_for(ip).await?
    {
        let minutes = retry_after.div_ceil(60);
        return Err(ApiError::rate_limit(Some(retry_after)).with_message(format!(
            "Terlalu banyak permintaan login dari IP ini. Silakan coba kembali dalam {minutes} menit."
        )));
    }
    if let Some(remaining) = limiter.locked_for(&email).await? {
        return Err(ApiError::locked(remaining));
    }

    let user = user::find_by_email(&state.db, &email, group_uid.as_deref()).await?;
    // Unknown user and wrong password are indistinguishable to the caller, in time as well.
    let verified = verify_login(&state.db, user.as_ref(), &password).await;
    let Some(user) = user.filter(|_| verified) else {
        limiter.record_failure(&email, ip).await?;
        return Err(ApiError::auth());
    };
    limiter.reset(&email).await?;

    if let Some(requested) = &group_uid
        && user.group(requested).is_none()
    {
        return Err(forbidden(MSG_NOT_MEMBER, ErrorCode::AuthForbidden));
    }
    let Some(group) = user.active_group.as_ref().filter(|g| g.is_joined()) else {
        return Err(inactive());
    };

    let tokens = start_session(&state, &user, group).await?;
    ok(LoginData { user, tokens }, "Login berhasil")
}

pub async fn refresh(
    State(state): State<AppState>,
    ValidatedJson(body): ValidatedJson<RefreshBody>,
) -> ApiResult<Tokens> {
    let Some(raw_token) = clean_string(Some(&body.refresh_token)) else {
        return Err(ApiError::auth()
            .with_status(StatusCode::BAD_REQUEST)
            .with_code(ErrorCode::AuthTokenInvalid)
            .with_message("Refresh token wajib disertakan"));
    };
    let token_hash = hash_token(&raw_token);
    let lock_value = uuid::Uuid::new_v4().to_string();
    let sessions = Sessions::new(state.redis.clone());

    // Concurrent refreshes of the same token would otherwise both rotate it.
    if !sessions
        .acquire_lock(&token_hash, &lock_value, LOCK_SECONDS)
        .await?
    {
        return Err(ApiError::rate_limit(None).with_message(
            "Permintaan pembaruan sesi sedang diproses. Silakan coba beberapa saat lagi.",
        ));
    }
    let result = rotate(&state, &sessions, &token_hash).await;
    // The lock expires on its own, so a failed release is not worth failing the request for.
    if let Err(err) = sessions.release_lock(&token_hash, &lock_value).await {
        tracing::warn!(error = %err, "could not release refresh lock");
    }
    ok(result?, "Token berhasil diperbarui")
}

async fn rotate(
    state: &AppState,
    sessions: &Sessions,
    token_hash: &str,
) -> Result<Tokens, ApiError> {
    let Some(existing) = sessions.get(token_hash).await? else {
        // A client that retried right after a rotation gets the same tokens back.
        if let Some(tokens) = sessions.grace_tokens(token_hash).await? {
            return Ok(tokens);
        }
        if let Some(family_id) = sessions.rotated_family(token_hash).await? {
            tracing::warn!("[Security Alert] Refresh token reuse detected for family {family_id}!");
            sessions.revoke_family(&family_id).await?;
            return Err(unauthorized(
                "Sesi Anda dihentikan demi keamanan karena terdeteksi anomali token. Silakan login kembali.",
                ErrorCode::AuthReuseDetected,
            ));
        }
        return Err(unauthorized(
            "Sesi tidak ditemukan atau telah kedaluwarsa",
            ErrorCode::AuthTokenInvalid,
        ));
    };

    if chrono::Utc::now().timestamp_millis() > existing.expires_at.saturating_mul(1000) {
        sessions.delete(token_hash, Some(&existing.user_id)).await?;
        return Err(unauthorized(
            "Sesi telah kedaluwarsa. Silakan login kembali.",
            ErrorCode::AuthTokenExpired,
        ));
    }

    let user = user::find_by_id(
        &state.db,
        &existing.user_id,
        existing.active_group_uid.as_deref(),
    )
    .await?
    .ok_or_else(user_not_found)?;
    let Some(group) = user.active_group.as_ref().filter(|g| g.is_joined()) else {
        return Err(inactive());
    };

    let (tokens, new_hash) = mint_tokens(state, &user, group).await?;
    let next = new_session(
        state,
        &user.id,
        new_hash,
        existing.family_id,
        &group.group_uid,
    );
    if !sessions.rotate(token_hash, &next, &tokens).await? {
        return Err(unauthorized(
            "Gagal melakukan rotasi token sesi",
            ErrorCode::AuthTokenInvalid,
        ));
    }
    Ok(tokens)
}

pub async fn switch_group(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<SwitchGroupBody>,
) -> ApiResult<LoginData> {
    let caller = auth.require_auth()?;
    let Some(group_uid) = clean_string(Some(&body.group_uid)) else {
        return Err(ApiError::validation().with_message("Group UID wajib disertakan"));
    };

    let user = user::find_by_id(&state.db, &caller.sub, Some(&group_uid))
        .await?
        .ok_or_else(user_not_found)?;
    let group = user
        .group(&group_uid)
        .ok_or_else(|| forbidden(MSG_NOT_MEMBER, ErrorCode::AuthForbidden))?;
    if !group.is_joined() {
        return Err(inactive());
    }

    let tokens = start_session(&state, &user, group).await?;
    ok(LoginData { user, tokens }, "Grup aktif berhasil diperbarui")
}

/// Always succeeds: revokes the given refresh token and, when the caller's access token is valid,
/// bumps the session version so their other access tokens stop working.
pub async fn logout(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<LogoutBody>,
) -> ApiResult<()> {
    let user_id = match &auth {
        Auth::User(user) => Some(user.sub.as_str()),
        Auth::Anonymous | Auth::Stale => None,
    };
    if let Some(raw_token) = clean_string(body.refresh_token.as_deref()) {
        Sessions::new(state.redis.clone())
            .delete(&hash_token(&raw_token), user_id)
            .await?;
    }
    if let Some(user_id) = user_id {
        state.sessions.increment_version(user_id).await?;
    }
    ok((), "Logout berhasil")
}
