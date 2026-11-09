//! Access token signing and verification, ported from `token.service.ts`.
//!
//! Tokens are RS256 JWTs with issuer `kawalo_auth` and audience `kawalo_mobile`; a token minted
//! here verifies in the TypeScript server and the other way round.

use std::time::{SystemTime, UNIX_EPOCH};

use jsonwebtoken::{Algorithm, Header, Validation, decode, decode_header, encode};
use serde::Serialize;
use serde_json::{Map, Value};
use sha2::{Digest, Sha256};

use crate::core::clean::{clean_integer, clean_string};
use crate::core::constants::{JWT_AUDIENCE, JWT_ISSUER};
use crate::core::error::ApiError;
use crate::core::security::rsa_key::{KeyError, RsaKeyManager};

#[derive(Debug, thiserror::Error)]
pub enum TokenError {
    #[error("session_ver must be a positive integer to issue an access token, got {0}")]
    InvalidSessionVersion(i64),
    #[error(transparent)]
    Key(#[from] KeyError),
    #[error("signing failed: {0}")]
    Sign(#[from] jsonwebtoken::errors::Error),
}

impl From<TokenError> for ApiError {
    fn from(err: TokenError) -> Self {
        tracing::error!(error = %err, "token error");
        Self::internal(err)
    }
}

/// Claims carried by an access token, both for signing and as the result of verification.
#[derive(Debug, Clone, Default, PartialEq, Eq, Serialize)]
pub struct JwtUserPayload {
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sub: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub email: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub name: Option<String>,
    /// Upper-cased by [`verify_access_token`].
    #[serde(skip_serializing_if = "Option::is_none")]
    pub role: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub group_uid: Option<String>,
    pub allow_admin: bool,
    pub allow_patrol: bool,
    pub allow_mon_patrol: bool,
    pub session_ver: i64,
}

/// A freshly signed token and its lifetime in seconds.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SignedToken {
    pub token: String,
    pub expires_in: u64,
}

#[derive(Serialize)]
struct Claims<'a> {
    #[serde(flatten)]
    user: &'a JwtUserPayload,
    iss: &'static str,
    aud: &'static str,
    iat: u64,
    exp: u64,
}

/// Issues an access token valid for `expires_in` seconds.
///
/// # Errors
/// [`TokenError::InvalidSessionVersion`] unless `payload.session_ver > 0`, or a key or signing
/// error.
pub fn sign_access_token(
    keys: &RsaKeyManager,
    expires_in: u64,
    payload: &JwtUserPayload,
) -> Result<SignedToken, TokenError> {
    if payload.session_ver <= 0 {
        return Err(TokenError::InvalidSessionVersion(payload.session_ver));
    }
    let signing = keys.signing_key()?;
    let mut header = Header::new(Algorithm::RS256);
    header.kid = Some(signing.kid);

    let iat = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs();
    let claims = Claims {
        user: payload,
        iss: JWT_ISSUER,
        aud: JWT_AUDIENCE,
        iat,
        exp: iat + expires_in,
    };
    let token = encode(&header, &claims, &signing.key)?;
    Ok(SignedToken { token, expires_in })
}

/// Verifies signature, issuer, audience and expiry; `None` for any failure.
///
/// A token must carry a positive integer `session_ver`. Text claims are trimmed and cleaned like
/// the TypeScript version, `role` is upper-cased and the `allow_*` flags use JavaScript
/// truthiness.
pub fn verify_access_token(keys: &RsaKeyManager, token: &str) -> Option<JwtUserPayload> {
    let kid = decode_header(token).ok()?.kid;
    let key = keys.verifying_key(kid.as_deref()).ok()?;

    let mut validation = Validation::new(Algorithm::RS256);
    validation.set_issuer(&[JWT_ISSUER]);
    validation.set_audience(&[JWT_AUDIENCE]);
    validation.set_required_spec_claims(&["exp", "iss", "aud"]);
    validation.validate_nbf = true;
    validation.leeway = 0;
    // jose rejects `exp <= now`; jsonwebtoken alone only rejects `exp < now`.
    validation.reject_tokens_expiring_in_less_than = 1;
    let claims = decode::<Map<String, Value>>(token, &key, &validation)
        .ok()?
        .claims;

    let session_ver = match claims.get("session_ver")? {
        Value::Number(n) => n
            .as_f64()
            .filter(|v| v.is_finite())
            .map(|v| v.trunc() as i64),
        Value::String(s) => clean_integer(Some(s)),
        _ => None,
    }
    .filter(|&v| v > 0)?;

    // A non-string, non-null claim makes JavaScript's `cleanString` throw, failing verification.
    let text = |name: &str| -> Option<Option<String>> {
        match claims.get(name) {
            None | Some(Value::Null) => Some(None),
            Some(Value::String(s)) => Some(clean_string(Some(s))),
            Some(_) => None,
        }
    };
    let truthy = |name: &str| match claims.get(name) {
        None | Some(Value::Null) => false,
        Some(Value::Bool(b)) => *b,
        Some(Value::Number(n)) => n.as_f64().is_some_and(|v| v != 0.0 && !v.is_nan()),
        Some(Value::String(s)) => !s.is_empty(),
        Some(_) => true,
    };

    Some(JwtUserPayload {
        sub: text("sub")?,
        email: text("email")?,
        name: text("name")?,
        role: text("role")?.map(|r| r.to_uppercase()),
        group_uid: text("group_uid")?,
        allow_admin: truthy("allow_admin"),
        allow_patrol: truthy("allow_patrol"),
        allow_mon_patrol: truthy("allow_mon_patrol"),
        session_ver,
    })
}

/// Opaque 64-character hex refresh token from 32 CSPRNG bytes.
pub fn generate_refresh_token() -> String {
    hex::encode(rand::random::<[u8; 32]>())
}

/// SHA-256 hex digest under which a refresh token is stored.
pub fn hash_token(raw_token: &str) -> String {
    hex::encode(Sha256::digest(raw_token.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;
    use base64::Engine;
    use base64::engine::general_purpose::URL_SAFE_NO_PAD;

    fn keys() -> (tempfile::TempDir, RsaKeyManager) {
        let dir = tempfile::tempdir().expect("tempdir");
        let keys = RsaKeyManager::init_with_modulus(dir.path(), 2048).expect("init");
        (dir, keys)
    }

    fn payload() -> JwtUserPayload {
        JwtUserPayload {
            sub: Some("usr-1".into()),
            email: Some("andi@x.id".into()),
            name: Some("Andi".into()),
            role: Some("guard".into()),
            group_uid: Some("g1".into()),
            allow_admin: true,
            session_ver: 3,
            ..JwtUserPayload::default()
        }
    }

    fn claims_of(token: &str) -> Value {
        let body = token.split('.').nth(1).expect("payload part");
        serde_json::from_slice(&URL_SAFE_NO_PAD.decode(body).expect("base64")).expect("json")
    }

    #[test]
    fn round_trip_uppercases_role_and_keeps_claims() {
        let (_dir, keys) = keys();
        let signed = sign_access_token(&keys, 900, &payload()).expect("sign");
        assert_eq!(signed.expires_in, 900);
        let verified = verify_access_token(&keys, &signed.token).expect("verify");
        assert_eq!(verified.role.as_deref(), Some("GUARD"));
        assert_eq!(verified.sub.as_deref(), Some("usr-1"));
        assert!(verified.allow_admin && !verified.allow_patrol);
        assert_eq!(verified.session_ver, 3);
    }

    #[test]
    fn claims_and_header_match_the_contract() {
        let (_dir, keys) = keys();
        let token = sign_access_token(&keys, 900, &payload())
            .expect("sign")
            .token;
        let claims = claims_of(&token);
        assert_eq!(claims["iss"], "kawalo_auth");
        assert_eq!(claims["aud"], "kawalo_mobile");
        assert_eq!(
            claims["exp"].as_u64().expect("exp") - claims["iat"].as_u64().expect("iat"),
            900
        );
        assert_eq!(claims["group_uid"], "g1");
        assert_eq!(claims["session_ver"], 3);
        let header = decode_header(&token).expect("header");
        assert_eq!(header.alg, Algorithm::RS256);
        assert_eq!(header.kid.as_deref(), Some(keys.active_kid().as_str()));
    }

    #[test]
    fn non_positive_session_version_is_refused() {
        let (_dir, keys) = keys();
        for ver in [0, -1] {
            let bad = JwtUserPayload {
                session_ver: ver,
                ..payload()
            };
            assert!(
                matches!(sign_access_token(&keys, 900, &bad), Err(TokenError::InvalidSessionVersion(v)) if v == ver)
            );
        }
    }

    #[test]
    fn tampered_expired_and_foreign_tokens_fail() {
        let (_dir, keys) = keys();
        let token = sign_access_token(&keys, 900, &payload())
            .expect("sign")
            .token;
        let mut parts: Vec<&str> = token.split('.').collect();
        let forged_body =
            URL_SAFE_NO_PAD.encode(r#"{"sub":"evil","email":"e@x.id","session_ver":1}"#);
        parts[1] = &forged_body;
        assert!(verify_access_token(&keys, &parts.join(".")).is_none());
        assert!(verify_access_token(&keys, "not.a.jwt").is_none());

        let expired = sign_access_token(&keys, 0, &payload()).expect("sign").token;
        assert!(verify_access_token(&keys, &expired).is_none());

        let (_other_dir, other) = self::keys();
        let foreign = sign_access_token(&other, 900, &payload())
            .expect("sign")
            .token;
        assert!(verify_access_token(&keys, &foreign).is_none());
    }

    #[test]
    fn retired_key_still_verifies_after_rotation() {
        let (_dir, keys) = keys();
        let before = sign_access_token(&keys, 900, &payload())
            .expect("sign")
            .token;
        keys.rotate_key(Some("kawalo-key-9")).expect("rotate");
        assert!(verify_access_token(&keys, &before).is_some());
        let after = sign_access_token(&keys, 900, &payload())
            .expect("sign")
            .token;
        assert_eq!(
            decode_header(&after).expect("header").kid.as_deref(),
            Some("kawalo-key-9")
        );
        assert!(verify_access_token(&keys, &after).is_some());
    }

    #[test]
    fn refresh_token_and_hash_helpers() {
        let token = generate_refresh_token();
        assert_eq!(token.len(), 64);
        assert!(token.bytes().all(|b| b.is_ascii_hexdigit()));
        assert_ne!(token, generate_refresh_token());
        assert_eq!(
            hash_token("abc"),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );
    }
}
