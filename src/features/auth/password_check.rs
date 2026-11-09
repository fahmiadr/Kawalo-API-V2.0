//! Password check at login, with a shadow Argon2id hash kept in `tkuser.PasswordHash`.
//!
//! While the legacy plain-text `Password` is filled it is the source of truth, because other
//! systems may still change it; the hash is a copy that is refreshed whenever the plain text
//! proves it stale. Only when `Password` is emptied (phase 2) does the hash decide alone.

use sqlx::MySqlPool;

use crate::core::security::password::{
    dummy_verify_blocking, hash_password_blocking, verify_password_blocking,
};
use crate::features::auth::user::{Secret, User};

/// Whether `password` (already trimmed) is `user`'s password; `None` stands for an unknown e-mail.
///
/// | hash matches | plain text      | outcome                                   |
/// |--------------|-----------------|-------------------------------------------|
/// | yes          | empty           | accept (phase 2)                          |
/// | yes          | matches         | accept                                    |
/// | yes          | differs         | reject: the hash is stale                 |
/// | no / absent  | matches         | accept, then (re)store the hash           |
/// | no / absent  | differs / empty | reject                                    |
///
/// Every failure costs one Argon2 verification, real or dummy, so response time does not tell an
/// unknown account from a wrong password. Storing the hash can fail without failing the login.
pub async fn verify_login(db: &MySqlPool, user: Option<&User>, password: &str) -> bool {
    let Some(user) = user else {
        dummy_verify_blocking(password.to_owned()).await;
        return false;
    };
    let credential = &user.credential;
    let plaintext = credential.plaintext.as_ref();
    let plaintext_matches = plaintext.map(|stored| stored.matches(password));

    let mut hash_matches = false;
    let mut verified_a_hash = false;
    if let Some(hash) = &credential.hash {
        match verify_password_blocking(hash.expose().to_owned(), password.to_owned()).await {
            Ok(matched) => {
                hash_matches = matched;
                verified_a_hash = true;
            }
            Err(err) => {
                tracing::warn!(user_id = %user.id, error = %err, "cannot verify password hash");
            }
        }
    }

    match (hash_matches, plaintext_matches) {
        (true, None | Some(true)) => return true,
        (_, Some(true)) => {
            if let Some(stored) = plaintext {
                store_hash(db, user, stored, password).await;
            }
            return true;
        }
        _ => {}
    }
    if !verified_a_hash {
        dummy_verify_blocking(password.to_owned()).await;
    }
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
    let result =
        sqlx::query("UPDATE tkuser SET PasswordHash = ? WHERE UserId = ? AND BINARY Password = ?")
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
