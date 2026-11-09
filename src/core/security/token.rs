//! Access token signing and verification, ported from `token.service.ts`.
//!
//! Tokens are RS256 JWTs with issuer `kawalo_auth` and audience `kawalo_mobile`; a token minted
//! here verifies in the TypeScript server and the other way round.

use std::time::{SystemTime, UNIX_EPOCH};

use std::num::NonZeroUsize;
use std::sync::LazyLock;
use std::sync::atomic::{AtomicU64, Ordering};
use std::thread;

use tokio::sync::Semaphore;

use aws_lc_rs::error::Unspecified;
use aws_lc_rs::rand::SystemRandom;
use aws_lc_rs::signature::RSA_PKCS1_SHA256;
use base64::Engine;
use base64::engine::general_purpose::URL_SAFE_NO_PAD;
use dashmap::DashMap;
use jsonwebtoken::{Algorithm, Validation, decode, decode_header};
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
    #[error("signing failed")]
    Sign(Unspecified),
    #[error("serialising the token failed: {0}")]
    Encode(#[from] serde_json::Error),
    #[error("signing task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
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
struct JwtHeader<'a> {
    typ: &'static str,
    alg: &'static str,
    kid: &'a str,
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

/// Signatures running at once; more would only time-share the cores and stretch every one of them.
static SIGNING_SLOTS: LazyLock<Semaphore> = LazyLock::new(|| {
    Semaphore::new(
        thread::available_parallelism()
            .map_or(2, NonZeroUsize::get)
            .max(2),
    )
});

/// [`sign_access_token`] on a blocking thread, so the 3 ms of RSA-4096 work does not stall the
/// async workers; waiting callers are served in arrival order.
///
/// # Errors
/// Same as [`sign_access_token`], plus [`TokenError::Join`] if the signing task panicked.
pub async fn sign_access_token_blocking(
    keys: &RsaKeyManager,
    expires_in: u64,
    payload: JwtUserPayload,
) -> Result<SignedToken, TokenError> {
    let _slot = SIGNING_SLOTS
        .acquire()
        .await
        .expect("the signing semaphore is never closed");
    let keys = keys.clone();
    tokio::task::spawn_blocking(move || sign_access_token(&keys, expires_in, &payload)).await?
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
    let header = JwtHeader {
        typ: "JWT",
        alg: "RS256",
        kid: &signing.kid,
    };

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
    let mut token = String::with_capacity(1024);
    URL_SAFE_NO_PAD.encode_string(serde_json::to_vec(&header)?, &mut token);
    token.push('.');
    URL_SAFE_NO_PAD.encode_string(serde_json::to_vec(&claims)?, &mut token);
    let mut signature = vec![0; signing.key.public_modulus_len()];
    signing
        .key
        .sign(
            &RSA_PKCS1_SHA256,
            &SystemRandom::new(),
            token.as_bytes(),
            &mut signature,
        )
        .map_err(TokenError::Sign)?;
    token.push('.');
    URL_SAFE_NO_PAD.encode_string(signature, &mut token);
    Ok(SignedToken { token, expires_in })
}

/// Verifies signature, issuer, audience and expiry; `None` for any failure.
///
/// A token must carry a positive integer `session_ver`. Text claims are trimmed and cleaned like
/// the TypeScript version, `role` is upper-cased and the `allow_*` flags use JavaScript
/// truthiness.
pub fn verify_access_token(keys: &RsaKeyManager, token: &str) -> Option<JwtUserPayload> {
    let cache = keys.verified_tokens();
    let digest: [u8; 32] = Sha256::digest(token.as_bytes()).into();
    let epoch = cache.epoch();
    if let Some(payload) = cache.get(&digest, epoch) {
        return Some(payload);
    }
    let (payload, exp) = verify_signed(keys, token)?;
    if let Some(exp) = exp {
        cache.insert(digest, epoch, exp, payload.clone());
    }
    Some(payload)
}

/// Full verification; also returns the token's `exp` so the result can be cached.
fn verify_signed(keys: &RsaKeyManager, token: &str) -> Option<(JwtUserPayload, Option<u64>)> {
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

    let exp = claims.get("exp").and_then(Value::as_u64);
    let payload = JwtUserPayload {
        sub: text("sub")?,
        email: text("email")?,
        name: text("name")?,
        role: text("role")?.map(|r| r.to_uppercase()),
        group_uid: text("group_uid")?,
        allow_admin: truthy("allow_admin"),
        allow_patrol: truthy("allow_patrol"),
        allow_mon_patrol: truthy("allow_mon_patrol"),
        session_ver,
    };
    Some((payload, exp))
}

/// Verified tokens keyed by the SHA-256 of the raw token, so the RSA verification (about 50 us
/// for a 4096-bit key) runs once per token instead of once per request.
///
/// An entry is only a signature result: callers still check the session version on every request.
/// It expires with the token's own `exp`, and a change to the key set invalidates everything
/// through the epoch.
#[derive(Debug, Default)]
pub(crate) struct VerifiedTokens {
    entries: DashMap<[u8; 32], Verified>,
    epoch: AtomicU64,
}

#[derive(Debug)]
struct Verified {
    payload: JwtUserPayload,
    exp: u64,
    epoch: u64,
}

/// Soft bound on cached tokens; one eviction pass runs when it is reached.
const MAX_VERIFIED_TOKENS: usize = 4096;

impl VerifiedTokens {
    pub(crate) fn epoch(&self) -> u64 {
        self.epoch.load(Ordering::Acquire)
    }

    pub(crate) fn invalidate(&self) {
        self.epoch.fetch_add(1, Ordering::AcqRel);
    }

    fn get(&self, digest: &[u8; 32], epoch: u64) -> Option<JwtUserPayload> {
        let entry = self.entries.get(digest)?;
        // jose rejects `exp <= now`, so the boundary second is already expired.
        (entry.epoch == epoch && entry.exp > now_secs()).then(|| entry.payload.clone())
    }

    fn insert(&self, digest: [u8; 32], epoch: u64, exp: u64, payload: JwtUserPayload) {
        if self.entries.len() >= MAX_VERIFIED_TOKENS {
            let now = now_secs();
            self.entries.retain(|_, v| v.epoch == epoch && v.exp > now);
            if self.entries.len() >= MAX_VERIFIED_TOKENS {
                self.entries.clear();
            }
        }
        self.entries.insert(
            digest,
            Verified {
                payload,
                exp,
                epoch,
            },
        );
    }
}

fn now_secs() -> u64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .unwrap_or_default()
        .as_secs()
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
    fn repeated_verification_is_served_from_the_cache_and_matches() {
        let (_dir, keys) = keys();
        let token = sign_access_token(&keys, 900, &payload())
            .expect("sign")
            .token;
        let first = verify_access_token(&keys, &token).expect("verify");
        assert_eq!(keys.verified_tokens().entries.len(), 1);
        assert_eq!(verify_access_token(&keys, &token), Some(first));
        assert!(verify_access_token(&keys, "not.a.jwt").is_none());
        assert_eq!(keys.verified_tokens().entries.len(), 1);
    }

    #[test]
    fn cached_entries_expire_with_the_token_and_die_with_the_key_set() {
        let cache = VerifiedTokens::default();
        let (live, dead, epoch) = ([1; 32], [2; 32], cache.epoch());
        cache.insert(live, epoch, now_secs() + 60, payload());
        cache.insert(dead, epoch, now_secs(), payload());
        assert!(cache.get(&live, epoch).is_some());
        assert!(cache.get(&dead, epoch).is_none(), "exp <= now is expired");

        cache.invalidate();
        assert!(cache.get(&live, cache.epoch()).is_none());

        cache.insert(live, epoch, now_secs() + 60, payload());
        assert!(
            cache.get(&live, cache.epoch()).is_none(),
            "a verification that started before the change must not be served after it"
        );
    }

    #[tokio::test]
    async fn concurrent_blocking_signatures_all_verify() {
        let (_dir, keys) = keys();
        let signing: Vec<_> = (0..12)
            .map(|n| {
                let keys = keys.clone();
                let payload = JwtUserPayload {
                    sub: Some(format!("usr-{n}")),
                    ..payload()
                };
                tokio::spawn(async move { sign_access_token_blocking(&keys, 900, payload).await })
            })
            .collect();
        for (n, task) in signing.into_iter().enumerate() {
            let token = task.await.expect("task").expect("sign").token;
            let verified = verify_access_token(&keys, &token).expect("verify");
            assert_eq!(verified.sub, Some(format!("usr-{n}")));
        }
    }

    #[test]
    fn rotating_keys_invalidates_cached_verifications() {
        let (_dir, keys) = keys();
        let token = sign_access_token(&keys, 900, &payload())
            .expect("sign")
            .token;
        assert!(verify_access_token(&keys, &token).is_some());
        let epoch = keys.verified_tokens().epoch();
        keys.rotate_key(Some("kawalo-key-7")).expect("rotate");
        assert_ne!(keys.verified_tokens().epoch(), epoch);
        assert!(verify_access_token(&keys, &token).is_some());
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
