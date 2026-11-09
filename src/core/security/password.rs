//! Argon2id password hashing for the `tkuser.PasswordHash` column.
//!
//! Hashing is memory-hard (about 19 MiB and tens of milliseconds per call), so the async entry
//! points run it on blocking threads behind a semaphore: it never stalls the runtime and the
//! memory in flight stays bounded.

use std::num::NonZeroUsize;
use std::sync::LazyLock;
use std::thread;

use argon2::Argon2;
use argon2::password_hash::{PasswordHasher, PasswordVerifier};
use tokio::sync::Semaphore;

/// Prefix of the PHC strings this module writes.
pub const PHC_PREFIX: &str = "$argon2id$";

#[derive(Debug, thiserror::Error)]
pub enum PasswordError {
    #[error("hashing failed: {0}")]
    Hash(argon2::password_hash::Error),
    #[error("stored hash is not a valid PHC string: {0}")]
    MalformedHash(String),
    #[error("hashing task failed: {0}")]
    Join(#[from] tokio::task::JoinError),
}

/// Hashes run at once; each holds its working memory for the whole call.
static SLOTS: LazyLock<Semaphore> = LazyLock::new(|| {
    Semaphore::new(
        thread::available_parallelism()
            .map_or(2, NonZeroUsize::get)
            .max(2),
    )
});

/// What unknown accounts are verified against, so they cost the same as real ones. Built with
/// the current parameters, so it follows them when they are raised.
static DUMMY_HASH: LazyLock<String> = LazyLock::new(|| {
    hash_password("kawalo-dummy-password").expect("hashing a constant with default parameters")
});

// `Argon2::default()` is Argon2id v19 with m=19456 KiB, t=2, p=1: the OWASP minimum. Stored PHC
// strings carry their own parameters, so raising them later only affects new hashes.
fn argon2() -> Argon2<'static> {
    Argon2::default()
}

/// Hashes `password` into a PHC string with a fresh random salt. Blocking: see
/// [`hash_password_blocking`] from async code.
///
/// # Errors
/// [`PasswordError::Hash`] when the system random source fails.
pub fn hash_password(password: &str) -> Result<String, PasswordError> {
    argon2()
        .hash_password(password.as_bytes())
        .map(|hash| hash.to_string())
        .map_err(PasswordError::Hash)
}

/// Checks `password` against a PHC string, using the parameters stored in it. Blocking.
///
/// # Errors
/// [`PasswordError::MalformedHash`] when `phc` cannot be parsed.
pub fn verify_password(phc: &str, password: &str) -> Result<bool, PasswordError> {
    use argon2::password_hash::{Error, phc::PasswordHash};

    let parsed =
        PasswordHash::new(phc).map_err(|err| PasswordError::MalformedHash(err.to_string()))?;
    match argon2().verify_password(password.as_bytes(), &parsed) {
        Ok(()) => Ok(true),
        Err(Error::PasswordInvalid) => Ok(false),
        Err(err) => Err(PasswordError::MalformedHash(err.to_string())),
    }
}

/// [`hash_password`] on a blocking thread.
///
/// # Errors
/// As [`hash_password`], plus [`PasswordError::Join`].
pub async fn hash_password_blocking(password: String) -> Result<String, PasswordError> {
    let _slot = SLOTS
        .acquire()
        .await
        .expect("the hashing semaphore is never closed");
    tokio::task::spawn_blocking(move || hash_password(&password)).await?
}

/// [`verify_password`] on a blocking thread.
///
/// # Errors
/// As [`verify_password`], plus [`PasswordError::Join`].
pub async fn verify_password_blocking(
    phc: String,
    password: String,
) -> Result<bool, PasswordError> {
    let _slot = SLOTS
        .acquire()
        .await
        .expect("the hashing semaphore is never closed");
    tokio::task::spawn_blocking(move || verify_password(&phc, &password)).await?
}

/// Spends the time of a real verification and discards the outcome. Used where no hash was
/// checked, so response time does not tell known from unknown accounts.
pub async fn dummy_verify_blocking(password: String) {
    let _slot = SLOTS
        .acquire()
        .await
        .expect("the hashing semaphore is never closed");
    let result = tokio::task::spawn_blocking(move || verify_password(&DUMMY_HASH, &password)).await;
    if let Err(err) = result {
        tracing::warn!(error = %err, "dummy password verification task failed");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn hashes_with_owasp_minimum_parameters_and_unique_salts() {
        let a = hash_password("rahasia123").unwrap();
        let b = hash_password("rahasia123").unwrap();
        assert!(a.starts_with(PHC_PREFIX), "{a}");
        assert!(a.contains("$v=19$m=19456,t=2,p=1$"), "{a}");
        assert_ne!(a, b);
    }

    #[test]
    fn verifies_only_the_right_password() {
        let hash = hash_password("rahasia123").unwrap();
        assert!(verify_password(&hash, "rahasia123").unwrap());
        assert!(!verify_password(&hash, "rahasia124").unwrap());
        assert!(!verify_password(&hash, "").unwrap());
    }

    #[test]
    fn verification_uses_the_parameters_stored_in_the_hash() {
        // Cheap parameters, as an older or newer deployment might have written.
        let phc = "$argon2id$v=19$m=8,t=1,p=1$c29tZXNhbHRzb21lc2FsdA$\
                   0r6lYb0k0o3SrSvL1v0lN2gM8mS1rO0y2Yk7w7Vv8yA";
        assert!(!verify_password(phc, "rahasia123").unwrap());

        let params = argon2::Params::new(8, 1, 1, None).unwrap();
        let cheap = Argon2::new(argon2::Algorithm::Argon2id, argon2::Version::V0x13, params);
        let hash = cheap.hash_password(b"rahasia123").unwrap().to_string();
        assert!(hash.contains("m=8,t=1,p=1"), "{hash}");
        assert!(verify_password(&hash, "rahasia123").unwrap());
    }

    #[test]
    fn malformed_hashes_are_errors_not_mismatches() {
        for phc in ["", "rahasia123", "$argon2id$", "$argon2id$v=19$m=x$abc$def"] {
            assert!(
                matches!(
                    verify_password(phc, "rahasia123"),
                    Err(PasswordError::MalformedHash(_))
                ),
                "{phc:?}"
            );
        }
    }

    #[tokio::test]
    async fn blocking_variants_round_trip_and_dummy_verify_completes() {
        let hash = hash_password_blocking("rahasia123".to_owned())
            .await
            .unwrap();
        assert!(
            verify_password_blocking(hash.clone(), "rahasia123".to_owned())
                .await
                .unwrap()
        );
        assert!(
            !verify_password_blocking(hash, "salah".to_owned())
                .await
                .unwrap()
        );
        dummy_verify_blocking("apa saja".to_owned()).await;
        assert!(DUMMY_HASH.starts_with(PHC_PREFIX));
    }
}
