//! SQL for `tkincident` and `tkincidentreply`. Every incident query is scoped to the caller's
//! `GroupUID` and `DeletedAt IS NULL`, except [`find_any`], which re-reads after a write.

use chrono::{DateTime, Utc};
use sqlx::{FromRow, MySql, MySqlPool, QueryBuilder};

use super::dto::{parse_photos, photos_column};
use super::event::ReplyEntity;

// Macros rather than consts: sqlx only accepts string literals as SQL, which `concat!` can build.
macro_rules! incident_columns {
    () => {
        "IncidentUID, GroupUID, Title, Description, Category, GLat, GLong, Photos, Status, \
         CreatedByEmail, CreatedByName, ResolvedByEmail, ResolvedByName, ResolvedAt, CreatedAt, \
         UpdatedAt"
    };
}

macro_rules! reply_columns {
    () => {
        "ReplyUID, IncidentUID, SenderEmail, SenderName, SenderRole, Message, Photos, CreatedAt"
    };
}

#[derive(Debug, FromRow)]
pub struct IncidentRow {
    #[sqlx(rename = "IncidentUID")]
    pub id: String,
    #[sqlx(rename = "GroupUID")]
    pub group_uid: String,
    #[sqlx(rename = "Title")]
    pub title: String,
    #[sqlx(rename = "Description")]
    pub description: Option<String>,
    #[sqlx(rename = "Category")]
    pub category: String,
    #[sqlx(rename = "GLat")]
    pub glat: Option<f64>,
    #[sqlx(rename = "GLong")]
    pub glong: Option<f64>,
    #[sqlx(rename = "Photos")]
    pub photos: Option<String>,
    #[sqlx(rename = "Status")]
    pub status: String,
    #[sqlx(rename = "CreatedByEmail")]
    pub created_by: String,
    #[sqlx(rename = "CreatedByName")]
    pub created_by_name: Option<String>,
    #[sqlx(rename = "ResolvedByEmail")]
    pub resolved_by: Option<String>,
    #[sqlx(rename = "ResolvedByName")]
    pub resolved_by_name: Option<String>,
    #[sqlx(rename = "ResolvedAt")]
    pub resolved_at: Option<DateTime<Utc>>,
    #[sqlx(rename = "CreatedAt")]
    pub created_at: DateTime<Utc>,
    #[sqlx(rename = "UpdatedAt")]
    pub updated_at: DateTime<Utc>,
}

impl IncidentRow {
    pub fn is_resolved(&self) -> bool {
        self.status == "RESOLVED"
    }
}

#[derive(Debug, FromRow)]
struct ReplyRow {
    #[sqlx(rename = "ReplyUID")]
    id: String,
    #[sqlx(rename = "IncidentUID")]
    incident_id: String,
    #[sqlx(rename = "SenderEmail")]
    sender_email: String,
    #[sqlx(rename = "SenderName")]
    sender_name: String,
    #[sqlx(rename = "SenderRole")]
    sender_role: String,
    #[sqlx(rename = "Message")]
    message: String,
    #[sqlx(rename = "Photos")]
    photos: Option<String>,
    #[sqlx(rename = "CreatedAt")]
    created_at: DateTime<Utc>,
}

impl From<ReplyRow> for ReplyEntity {
    fn from(row: ReplyRow) -> Self {
        Self {
            id: row.id,
            incident_id: row.incident_id,
            sender_email: row.sender_email,
            sender_name: row.sender_name,
            sender_role: row.sender_role,
            message: row.message,
            photos: parse_photos(row.photos.as_deref()),
            created_at: row.created_at,
        }
    }
}

#[derive(Debug)]
pub struct Filter<'a> {
    pub status: Option<&'static str>,
    pub search: Option<&'a str>,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
    pub take: u32,
    pub skip: u32,
}

pub async fn list(
    db: &MySqlPool,
    group_uid: &str,
    filter: &Filter<'_>,
) -> sqlx::Result<Vec<IncidentRow>> {
    let mut query: QueryBuilder<MySql> = QueryBuilder::new("SELECT ");
    query
        .push(incident_columns!())
        .push(" FROM tkincident WHERE GroupUID = ")
        .push_bind(group_uid)
        .push(" AND DeletedAt IS NULL");
    if let Some(status) = filter.status {
        query.push(" AND Status = ").push_bind(status);
    }
    if let Some(start) = filter.start {
        query.push(" AND CreatedAt >= ").push_bind(start);
    }
    if let Some(end) = filter.end {
        query.push(" AND CreatedAt <= ").push_bind(end);
    }
    if let Some(search) = filter.search {
        let pattern = format!("%{search}%");
        query
            .push(" AND (Title LIKE ")
            .push_bind(pattern.clone())
            .push(" OR Description LIKE ")
            .push_bind(pattern.clone())
            .push(" OR CreatedByName LIKE ")
            .push_bind(pattern)
            .push(")");
    }
    // No tie-breaker on purpose: the SQL stays identical to what Prisma sent, so rows created in
    // the same second come back in the same order as before.
    query
        .push(" ORDER BY CreatedAt DESC LIMIT ")
        .push_bind(filter.take)
        .push(" OFFSET ")
        .push_bind(filter.skip);
    query.build_query_as().fetch_all(db).await
}

pub async fn find(db: &MySqlPool, id: &str, group_uid: &str) -> sqlx::Result<Option<IncidentRow>> {
    sqlx::query_as(concat!(
        "SELECT ",
        incident_columns!(),
        " FROM tkincident WHERE IncidentUID = ? AND GroupUID = ? AND DeletedAt IS NULL LIMIT 1"
    ))
    .bind(id)
    .bind(group_uid)
    .fetch_optional(db)
    .await
}

/// Re-reads an incident by id alone, including a soft-deleted one.
pub async fn find_any(db: &MySqlPool, id: &str) -> sqlx::Result<Option<IncidentRow>> {
    sqlx::query_as(concat!(
        "SELECT ",
        incident_columns!(),
        " FROM tkincident WHERE IncidentUID = ? LIMIT 1"
    ))
    .bind(id)
    .fetch_optional(db)
    .await
}

/// Replies oldest first; UUIDv7 ids sort by creation time.
pub async fn replies(db: &MySqlPool, incident_id: &str) -> sqlx::Result<Vec<ReplyEntity>> {
    let rows: Vec<ReplyRow> = sqlx::query_as(concat!(
        "SELECT ",
        reply_columns!(),
        " FROM tkincidentreply WHERE IncidentUID = ? ORDER BY ReplyUID ASC"
    ))
    .bind(incident_id)
    .fetch_all(db)
    .await?;
    Ok(rows.into_iter().map(ReplyEntity::from).collect())
}

pub async fn insert(db: &MySqlPool, row: &IncidentRow) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO tkincident (IncidentUID, GroupUID, Title, Description, Category, GLat, GLong, \
         Photos, CreatedByEmail, CreatedByName, CreatedAt, UpdatedAt) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&row.id)
    .bind(&row.group_uid)
    .bind(&row.title)
    .bind(&row.description)
    .bind(&row.category)
    .bind(row.glat)
    .bind(row.glong)
    .bind(&row.photos)
    .bind(&row.created_by)
    .bind(&row.created_by_name)
    .bind(row.created_at)
    .bind(row.updated_at)
    .execute(db)
    .await?;
    Ok(())
}

pub async fn insert_reply(db: &MySqlPool, reply: &ReplyEntity) -> sqlx::Result<()> {
    sqlx::query(
        "INSERT INTO tkincidentreply (ReplyUID, IncidentUID, SenderEmail, SenderName, SenderRole, \
         Message, Photos, CreatedAt) VALUES (?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(&reply.id)
    .bind(&reply.incident_id)
    .bind(&reply.sender_email)
    .bind(&reply.sender_name)
    .bind(&reply.sender_role)
    .bind(&reply.message)
    .bind(photos_column(&reply.photos))
    .bind(reply.created_at)
    .execute(db)
    .await?;
    Ok(())
}

/// Closes an incident that is still `OPEN`; returns whether this call was the one that closed it.
/// The `Status = 'OPEN'` filter makes the update atomic: of two concurrent resolves only one wins.
pub async fn resolve(
    db: &MySqlPool,
    id: &str,
    by_email: &str,
    by_name: &str,
    now: DateTime<Utc>,
) -> sqlx::Result<bool> {
    let result = sqlx::query(
        "UPDATE tkincident SET Status = 'RESOLVED', ResolvedByEmail = ?, ResolvedByName = ?, \
         ResolvedAt = ?, UpdatedAt = ? WHERE IncidentUID = ? AND Status = 'OPEN' AND DeletedAt IS NULL",
    )
    .bind(by_email)
    .bind(by_name)
    .bind(now)
    .bind(now)
    .bind(id)
    .execute(db)
    .await?;
    Ok(result.rows_affected() > 0)
}

pub async fn soft_delete(db: &MySqlPool, id: &str, now: DateTime<Utc>) -> sqlx::Result<()> {
    sqlx::query("UPDATE tkincident SET DeletedAt = ?, UpdatedAt = ? WHERE IncidentUID = ?")
        .bind(now)
        .bind(now)
        .bind(id)
        .execute(db)
        .await?;
    Ok(())
}
