use sqlx::MySqlPool;

pub async fn find(db: &MySqlPool, email: &str, group_uid: &str) -> sqlx::Result<Option<String>> {
    sqlx::query_scalar("SELECT Layout FROM tkhomelayout WHERE UserEmail = ? AND GroupUID = ?")
        .bind(email)
        .bind(group_uid)
        .fetch_optional(db)
        .await
}

pub async fn upsert(
    db: &MySqlPool,
    email: &str,
    group_uid: &str,
    layout: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO tkhomelayout (UserEmail, GroupUID, Layout, UpdatedAt) \
         VALUES (?, ?, ?, CURRENT_TIMESTAMP(0)) \
         ON DUPLICATE KEY UPDATE Layout = VALUES(Layout), UpdatedAt = CURRENT_TIMESTAMP(0)",
    )
    .bind(email)
    .bind(group_uid)
    .bind(layout)
    .execute(db)
    .await?;
    Ok(())
}
