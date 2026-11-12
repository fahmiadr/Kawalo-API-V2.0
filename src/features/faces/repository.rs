use chrono::{DateTime, Utc};
use sqlx::{FromRow, MySql, MySqlPool, QueryBuilder};

use crate::core::clean::clean_string;

/// Audit actions in `tkfaceaudit.Action`.
pub(crate) const AUDIT_REGISTER: &str = "REGISTER";
pub(crate) const AUDIT_DUPLICATE_BLOCKED: &str = "DUPLICATE_BLOCKED";
pub(crate) const AUDIT_DELETE_SELF: &str = "DELETE_SELF";
pub(crate) const AUDIT_DELETE_BY_ADMIN: &str = "DELETE_BY_ADMIN";
pub(crate) const AUDIT_VIEW_REGISTER_PHOTO: &str = "VIEW_REGISTER_PHOTO";
pub(crate) const AUDIT_VIEW_ATTENDANCE_PHOTO: &str = "VIEW_ATTENDANCE_PHOTO";

#[derive(Debug, FromRow)]
pub(crate) struct EmbeddingRow {
    /// As stored; it is also the AES-GCM associated data of `embedding`.
    pub email: String,
    pub embedding: Vec<u8>,
    pub model_version: String,
    pub photo_count: i32,
    pub created_at: DateTime<Utc>,
}

#[derive(Debug, FromRow)]
pub(crate) struct PhotoRow {
    pub id: String,
    pub created_at: DateTime<Utc>,
}

pub(crate) async fn find(db: &MySqlPool, email: &str) -> sqlx::Result<Option<EmbeddingRow>> {
    sqlx::query_as(
        "SELECT UserEmail AS email, Embedding AS embedding, ModelVersion AS model_version, \
         PhotoCount AS photo_count, CreatedAt AS created_at \
         FROM tkfaceembedding WHERE UserEmail = ?",
    )
    .bind(email)
    .fetch_optional(db)
    .await
}

/// Embeddings of the other joined members of the group. Two queries instead of a join, because
/// the legacy tables and the new one may not share a collation.
pub(crate) async fn group_embeddings(
    db: &MySqlPool,
    group_uid: &str,
    exclude_email: &str,
) -> sqlx::Result<Vec<EmbeddingRow>> {
    let emails: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT Email FROM tkusergroup WHERE GroupUID = ? AND Status = 'JOINED' AND Email <> ?",
    )
    .bind(group_uid)
    .bind(exclude_email)
    .fetch_all(db)
    .await?;
    let emails: Vec<String> = emails
        .iter()
        .filter_map(|e| clean_string(e.as_deref()))
        .collect();
    if emails.is_empty() {
        return Ok(Vec::new());
    }
    let mut query = QueryBuilder::<MySql>::new(
        "SELECT UserEmail AS email, Embedding AS embedding, ModelVersion AS model_version, \
         PhotoCount AS photo_count, CreatedAt AS created_at \
         FROM tkfaceembedding WHERE UserEmail IN (",
    );
    let mut list = query.separated(", ");
    for email in &emails {
        list.push_bind(email.as_str());
    }
    query.push(")");
    query.build_query_as().fetch_all(db).await
}

pub(crate) struct NewRegistration<'a> {
    pub email: &'a str,
    pub embedding: &'a [u8],
    pub model_version: &'a str,
    /// `(photo id, relative file path)`.
    pub photos: &'a [(String, String)],
    pub now: DateTime<Utc>,
}

/// Embedding and photo rows in one transaction. A unique violation means a concurrent
/// registration of the same user won.
pub(crate) async fn insert(db: &MySqlPool, new: &NewRegistration<'_>) -> sqlx::Result<()> {
    let mut tx = db.begin().await?;
    sqlx::query(
        "INSERT INTO tkfaceembedding (UserEmail, Embedding, ModelVersion, PhotoCount, CreatedAt) \
         VALUES (?, ?, ?, ?, ?)",
    )
    .bind(new.email)
    .bind(new.embedding)
    .bind(new.model_version)
    .bind(i32::try_from(new.photos.len()).unwrap_or(i32::MAX))
    .bind(new.now)
    .execute(&mut *tx)
    .await?;
    for (id, path) in new.photos {
        sqlx::query(
            "INSERT INTO tkfacephoto (PhotoUID, UserEmail, FilePath, CreatedAt) VALUES (?, ?, ?, ?)",
        )
        .bind(id)
        .bind(new.email)
        .bind(path)
        .bind(new.now)
        .execute(&mut *tx)
        .await?;
    }
    tx.commit().await
}

/// Deletes the embedding and registration photo rows; returns whether an embedding existed and
/// the photo files to remove afterwards.
pub(crate) async fn delete(db: &MySqlPool, email: &str) -> sqlx::Result<(bool, Vec<String>)> {
    let mut tx = db.begin().await?;
    let paths: Vec<String> =
        sqlx::query_scalar("SELECT FilePath FROM tkfacephoto WHERE UserEmail = ?")
            .bind(email)
            .fetch_all(&mut *tx)
            .await?;
    sqlx::query("DELETE FROM tkfacephoto WHERE UserEmail = ?")
        .bind(email)
        .execute(&mut *tx)
        .await?;
    let removed = sqlx::query("DELETE FROM tkfaceembedding WHERE UserEmail = ?")
        .bind(email)
        .execute(&mut *tx)
        .await?
        .rows_affected();
    tx.commit().await?;
    Ok((removed > 0 || !paths.is_empty(), paths))
}

pub(crate) async fn photos(db: &MySqlPool, email: &str) -> sqlx::Result<Vec<PhotoRow>> {
    sqlx::query_as(
        "SELECT PhotoUID AS id, CreatedAt AS created_at \
         FROM tkfacephoto WHERE UserEmail = ? ORDER BY CreatedAt, PhotoUID",
    )
    .bind(email)
    .fetch_all(db)
    .await
}

pub(crate) async fn photo(db: &MySqlPool, email: &str, id: &str) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT FilePath FROM tkfacephoto WHERE UserEmail = ? AND PhotoUID = ?")
        .bind(email)
        .bind(id)
        .fetch_optional(db)
        .await
}

/// Whether `email` is a member of the group. Viewing biometric data needs a current member;
/// deleting it also covers someone who has left (`KICKED`), so an admin can still clean up.
/// Pending invitations (`JOIN_WAIT`) and unknown statuses never count: otherwise an admin could
/// invite any user to reach their data.
pub(crate) async fn is_member(
    db: &MySqlPool,
    group_uid: &str,
    email: &str,
    include_left: bool,
) -> sqlx::Result<bool> {
    let found: Option<i32> = sqlx::query_scalar(
        "SELECT 1 FROM tkusergroup WHERE GroupUID = ? AND Email = ? \
         AND (Status = 'JOINED' OR (? AND Status = 'KICKED')) LIMIT 1",
    )
    .bind(group_uid)
    .bind(email)
    .bind(include_left)
    .fetch_optional(db)
    .await?;
    Ok(found.is_some())
}

pub(crate) struct Audit<'a> {
    pub action: &'a str,
    pub actor: &'a str,
    pub target: &'a str,
    pub group_uid: Option<&'a str>,
    pub reference: Option<&'a str>,
}

/// Callers choose: views fail closed when the audit row cannot be written, while actions that
/// already happened (deletions) only log the failure.
pub(crate) async fn audit(db: &MySqlPool, entry: Audit<'_>) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO tkfaceaudit (Action, ActorEmail, TargetEmail, GroupUID, Reference, CreatedAt) \
         VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(entry.action)
    .bind(entry.actor)
    .bind(entry.target)
    .bind(entry.group_uid)
    .bind(entry.reference)
    .bind(Utc::now())
    .execute(db)
    .await
    .map(drop)
}

/// [`audit`] for actions that already happened.
pub(crate) async fn audit_after(db: &MySqlPool, entry: Audit<'_>) {
    let action = entry.action;
    if let Err(err) = audit(db, entry).await {
        tracing::error!(error = %err, action, "[Face] could not write the audit row");
    }
}
