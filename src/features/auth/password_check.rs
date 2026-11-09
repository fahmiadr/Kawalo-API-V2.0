//! Password check at login, with transparent upgrade of plain-text passwords to Argon2id.

use sqlx::MySqlPool;

use crate::core::security::password::{
    dummy_verify_blocking, hash_password_blocking, verify_password_blocking,
};
use crate::features::auth::user::{Credential, Secret, User};

/// Whether `password` (already trimmed) is `user`'s password; `None` stands for an unknown e-mail.
///
/// Every failure costs one Argon2 verification, real or dummy, so response time does not tell an
/// unknown account from a wrong password. A plain-text match stores the Argon2id hash in
/// `PasswordHash`; storing it can fail without failing the login.
pub async fn verify_login(db: &MySqlPool, user: Option<&User>, password: &str) -> bool {
    let Some(user) = user else {
        dummy_verify_blocking(password.to_owned()).await;
        return false;
    };
    match &user.credential {
        Credential::Hash(phc) => {
            let verified =
                verify_password_blocking(phc.expose().to_owned(), password.to_owned()).await;
            match verified {
                Ok(matched) => return matched,
                // Fail closed: falling back to `Password` would let an old password back in.
                Err(err) => {
                    tracing::warn!(user_id = %user.id, error = %err, "cannot verify password hash")
                }
            }
        }
        Credential::Plaintext(stored) if stored.matches(password) => {
            store_hash(db, user, stored, password).await;
            return true;
        }
        Credential::Plaintext(_) => {}
    }
    dummy_verify_blocking(password.to_owned()).await;
    false
}

async fn store_hash(db: &MySqlPool, user: &User, stored: &Secret, password: &str) {
    let phc = match hash_password_blocking(password.to_owned()).await {
        Ok(phc) => phc,
        Err(err) => {
            tracing::warn!(user_id = %user.id, error = %err, "cannot hash password");
            return;
        }
    };
    // Matching the plain text again keeps a password changed elsewhere since it was read from
    // getting a stale hash; BINARY because the column's collation is case-insensitive.
    let result = sqlx::query(
        "UPDATE tkuser SET PasswordHash = ? \
         WHERE UserId = ? AND (PasswordHash IS NULL OR PasswordHash = '') AND BINARY Password = ?",
    )
    .bind(&phc)
    .bind(&user.id)
    .bind(stored.expose())
    .execute(db)
    .await;
    match result {
        Ok(done) if done.rows_affected() == 0 => {
            tracing::debug!(user_id = %user.id, "password hash not stored: row changed meanwhile");
        }
        Ok(_) => {}
        Err(err) => tracing::warn!(user_id = %user.id, error = %err, "cannot store password hash"),
    }
}
