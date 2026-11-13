use chrono::{DateTime, Utc};
use sqlx::{FromRow, MySqlPool};

#[derive(Debug, FromRow)]
pub struct Row {
    pub name: String,
    pub lat: f64,
    pub long: f64,
    pub radius: i32,
    pub updated_by: String,
    pub updated_at: DateTime<Utc>,
}

pub struct Point<'a> {
    pub name: &'a str,
    pub lat: f64,
    pub long: f64,
    pub radius: i32,
}

pub async fn find(db: &MySqlPool, group_uid: &str) -> sqlx::Result<Option<Row>> {
    sqlx::query_as(
        "SELECT LocName AS name, LocLat AS lat, LocLong AS `long`, LocRadius AS radius, \
         UpdatedBy AS updated_by, UpdatedAt AS updated_at \
         FROM tkattendancelocation WHERE GroupUID = ?",
    )
    .bind(group_uid)
    .fetch_optional(db)
    .await
}

pub async fn upsert(
    db: &MySqlPool,
    group_uid: &str,
    point: &Point<'_>,
    updated_by: &str,
) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO tkattendancelocation \
         (GroupUID, LocName, LocLat, LocLong, LocRadius, UpdatedBy, UpdatedAt) \
         VALUES (?, ?, ?, ?, ?, ?, CURRENT_TIMESTAMP(0)) \
         ON DUPLICATE KEY UPDATE LocName = VALUES(LocName), LocLat = VALUES(LocLat), \
         LocLong = VALUES(LocLong), LocRadius = VALUES(LocRadius), \
         UpdatedBy = VALUES(UpdatedBy), UpdatedAt = CURRENT_TIMESTAMP(0)",
    )
    .bind(group_uid)
    .bind(point.name)
    .bind(point.lat)
    .bind(point.long)
    .bind(point.radius)
    .bind(updated_by)
    .execute(db)
    .await?;
    Ok(())
}
