use chrono::{DateTime, Utc};
use sqlx::{FromRow, MySqlPool};

#[derive(Debug, FromRow)]
pub struct Row {
    pub percent: i32,
    pub updated_by: String,
    pub updated_at: DateTime<Utc>,
}

pub async fn find(db: &MySqlPool, group_uid: &str) -> sqlx::Result<Option<Row>> {
    sqlx::query_as(
        "SELECT FaceMatchPercent AS percent, UpdatedBy AS updated_by, UpdatedAt AS updated_at \
         FROM tkattendancesetting WHERE GroupUID = ?",
    )
    .bind(group_uid)
    .fetch_optional(db)
    .await
}

pub async fn upsert(
    db: &MySqlPool,
    group_uid: &str,
    percent: i32,
    updated_by: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO tkattendancesetting (GroupUID, FaceMatchPercent, UpdatedBy, UpdatedAt) \
         VALUES (?, ?, ?, CURRENT_TIMESTAMP(0)) \
         ON DUPLICATE KEY UPDATE FaceMatchPercent = VALUES(FaceMatchPercent), \
         UpdatedBy = VALUES(UpdatedBy), UpdatedAt = CURRENT_TIMESTAMP(0)",
    )
    .bind(group_uid)
    .bind(percent)
    .bind(updated_by)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn delete(db: &MySqlPool, group_uid: &str) -> sqlx::Result<()> {
    sqlx::query("DELETE FROM tkattendancesetting WHERE GroupUID = ?")
        .bind(group_uid)
        .execute(db)
        .await?;
    Ok(())
}
