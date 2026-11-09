//! SQL for `tkguest` and its lookups (`tkgroup`, `tkuser`, `tkusergroup`, `tkmasterguesttype`).

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Serialize;
use sqlx::{FromRow, MySql, MySqlPool, QueryBuilder};

use crate::core::clean::{Patch, clean_integer, clean_naive_datetime, clean_string};
use crate::core::ids::{generate_prefixed_uuid_v7, generate_uuid_v4};
use crate::core::pagination::Pagination;

use super::dto::MasterGuestType;

/// Dates are read as text: legacy rows may hold `0000-00-00 00:00:00`, which the binary protocol
/// cannot decode into a timestamp.
const GUEST_SELECT: &str = "SELECT CAST(g.GuestId AS CHAR) AS id, gr.GroupName AS group_name, \
    g.GuestName AS guest_name, g.GuestNumber AS guest_number, g.GuestIdentity AS guest_identity, \
    g.GuestIdentityType AS guest_identity_type, g.GuestPhoto AS guest_photo, \
    CAST(g.GuestDT AS CHAR) AS guest_dt, CAST(g.GuestAckDT AS CHAR) AS guest_ack_dt, \
    CAST(g.GuestOutDT AS CHAR) AS guest_out_dt, g.GuestPurpose AS guest_purpose, \
    g.GuestDestination AS guest_destination, g.GuestToUserEmail AS guest_to_user_email, \
    tu.UserName AS guest_to_user_name, g.GuestCreatorEmail AS guest_creator_email, \
    g.GuestCreatorName AS guest_creator_name, g.GuestCategory AS guest_category, \
    g.QRUID AS qr_uid, g.GuestAddress AS guest_address, g.GuestType AS guest_type, \
    g.GuestCheckOutByEmail AS guest_checkout_by_email, cu.UserName AS guest_checkout_by_name \
    FROM tkguest g \
    LEFT JOIN tkgroup gr ON gr.GroupUID = g.GuestGroupUID \
    LEFT JOIN tkuser tu ON tu.UserEmail = g.GuestToUserEmail \
    LEFT JOIN tkuser cu ON cu.UserEmail = g.GuestCheckOutByEmail";

const SEARCH_COLUMNS: [&str; 14] = [
    "GuestName",
    "GuestNumber",
    "GuestIdentityType",
    "GuestIdentity",
    "GuestCategory",
    "GuestType",
    "GuestAddress",
    "GuestPurpose",
    "GuestDestination",
    "GuestToUserEmail",
    "GuestCreatorEmail",
    "GuestCreatorName",
    "GuestCheckOutByEmail",
    "QRUID",
];

/// A guest as returned by the API. Field names double as SQL aliases and JSON keys.
#[derive(Debug, Clone, FromRow, Serialize)]
pub struct GuestItem {
    pub id: String,
    pub group_name: Option<String>,
    pub guest_name: Option<String>,
    pub guest_number: Option<String>,
    pub guest_identity: Option<String>,
    pub guest_identity_type: Option<String>,
    pub guest_photo: Option<String>,
    pub guest_dt: Option<String>,
    pub guest_ack_dt: Option<String>,
    pub guest_out_dt: Option<String>,
    pub guest_purpose: Option<String>,
    pub guest_destination: Option<String>,
    pub guest_to_user_email: Option<String>,
    pub guest_to_user_name: Option<String>,
    pub guest_creator_email: Option<String>,
    pub guest_creator_name: Option<String>,
    pub guest_category: Option<String>,
    pub qr_uid: Option<String>,
    pub guest_address: Option<String>,
    pub guest_type: Option<String>,
    pub guest_checkout_by_email: Option<String>,
    pub guest_checkout_by_name: Option<String>,
}

fn tidy(field: &mut Option<String>) {
    *field = clean_string(field.as_deref());
}

fn tidy_timestamp(field: &mut Option<String>) {
    *field = field
        .as_deref()
        .and_then(|text| NaiveDateTime::parse_from_str(text, "%Y-%m-%d %H:%M:%S").ok())
        .and_then(clean_naive_datetime);
}

impl GuestItem {
    /// Applies the legacy-data sanitising of the anti-corruption layer: dirty markers become
    /// `null`, emails are lower-cased, zero and epoch dates are dropped.
    fn sanitised(mut self) -> Self {
        for field in [
            &mut self.group_name,
            &mut self.guest_name,
            &mut self.guest_number,
            &mut self.guest_identity,
            &mut self.guest_identity_type,
            &mut self.guest_photo,
            &mut self.guest_purpose,
            &mut self.guest_destination,
            &mut self.guest_to_user_email,
            &mut self.guest_to_user_name,
            &mut self.guest_creator_email,
            &mut self.guest_creator_name,
            &mut self.guest_category,
            &mut self.qr_uid,
            &mut self.guest_address,
            &mut self.guest_type,
            &mut self.guest_checkout_by_email,
            &mut self.guest_checkout_by_name,
        ] {
            tidy(field);
        }
        for field in [
            &mut self.guest_dt,
            &mut self.guest_ack_dt,
            &mut self.guest_out_dt,
        ] {
            tidy_timestamp(field);
        }
        self
    }

    /// Numeric `GuestId`; the column is an auto-increment integer rendered as text.
    pub fn row_id(&self) -> i64 {
        self.id.parse().unwrap_or_default()
    }
}

/// Filters of `GET /guests`; absent filters are not applied.
#[derive(Debug)]
pub struct GuestFilter {
    pub search: Option<String>,
    pub page: Pagination,
    pub start: Option<DateTime<Utc>>,
    pub end: Option<DateTime<Utc>>,
    pub created_by: Option<String>,
    pub is_active: Option<bool>,
}

pub async fn list(
    pool: &MySqlPool,
    group_uid: &str,
    filter: &GuestFilter,
) -> Result<Vec<GuestItem>, sqlx::Error> {
    let mut query = QueryBuilder::<MySql>::new(GUEST_SELECT);
    query.push(" WHERE g.GuestGroupUID = ").push_bind(group_uid);
    if let Some(start) = filter.start {
        query.push(" AND g.GuestDT >= ").push_bind(start);
    }
    if let Some(end) = filter.end {
        query.push(" AND g.GuestDT <= ").push_bind(end);
    }
    let pattern = filter.search.as_ref().map(|search| format!("%{search}%"));
    if let Some(pattern) = &pattern {
        query.push(" AND (");
        for (index, column) in SEARCH_COLUMNS.iter().enumerate() {
            if index > 0 {
                query.push(" OR ");
            }
            query
                .push("g.")
                .push(column)
                .push(" LIKE ")
                .push_bind(pattern);
        }
        // Also match guests whose host or checkout officer has a matching name, limited to
        // joined members of the active group.
        query
            .push(
                " OR EXISTS (SELECT 1 FROM tkusergroup ug JOIN tkuser u ON u.UserEmail = ug.Email \
                 WHERE ug.GroupUID = ",
            )
            .push_bind(group_uid)
            .push(" AND ug.Status = 'JOINED' AND u.UserName LIKE ")
            .push_bind(pattern)
            .push(" AND u.UserEmail IN (g.GuestToUserEmail, g.GuestCheckOutByEmail)))");
    }
    if let Some(created_by) = &filter.created_by {
        query
            .push(" AND g.GuestCreatorEmail = ")
            .push_bind(created_by);
    }
    if let Some(active) = filter.is_active {
        query.push(if active {
            " AND g.GuestOutDT IS NULL"
        } else {
            " AND g.GuestOutDT IS NOT NULL"
        });
    }
    // No tie-breaker on purpose: rows within the same second must come back in the order the
    // TypeScript query produced.
    query.push(" ORDER BY g.GuestDT DESC");
    if let Pagination::Page { take, skip } = filter.page {
        query.push(" LIMIT ").push_bind(take);
        query.push(" OFFSET ").push_bind(skip);
    }
    let rows = query.build_query_as::<GuestItem>().fetch_all(pool).await?;
    Ok(rows.into_iter().map(GuestItem::sanitised).collect())
}

/// Guest of `group_uid` whose `QRUID` equals `identifier`, or whose numeric id does.
pub async fn find(
    pool: &MySqlPool,
    group_uid: &str,
    identifier: &str,
) -> Result<Option<GuestItem>, sqlx::Error> {
    let mut query = QueryBuilder::<MySql>::new(GUEST_SELECT);
    query
        .push(" WHERE g.GuestGroupUID = ")
        .push_bind(group_uid)
        .push(" AND (g.QRUID = ")
        .push_bind(identifier);
    if let Some(id) = clean_integer(Some(identifier)).filter(|&id| id > 0) {
        query.push(" OR g.GuestId = ").push_bind(id);
    }
    query.push(") LIMIT 1");
    let row = query
        .build_query_as::<GuestItem>()
        .fetch_optional(pool)
        .await?;
    Ok(row.map(GuestItem::sanitised))
}

pub async fn find_by_row_id(pool: &MySqlPool, id: i64) -> Result<Option<GuestItem>, sqlx::Error> {
    let mut query = QueryBuilder::<MySql>::new(GUEST_SELECT);
    query.push(" WHERE g.GuestId = ").push_bind(id);
    let row = query
        .build_query_as::<GuestItem>()
        .fetch_optional(pool)
        .await?;
    Ok(row.map(GuestItem::sanitised))
}

/// Creator emails of the guests of `group_uid` among `ids`, one entry per existing guest.
pub async fn creators(
    pool: &MySqlPool,
    group_uid: &str,
    ids: &[i64],
) -> Result<Vec<Option<String>>, sqlx::Error> {
    if ids.is_empty() {
        return Ok(Vec::new());
    }
    let mut query =
        QueryBuilder::<MySql>::new("SELECT GuestCreatorEmail FROM tkguest WHERE GuestId IN (");
    let mut list = query.separated(", ");
    for id in ids {
        list.push_bind(id);
    }
    query.push(") AND GuestGroupUID = ").push_bind(group_uid);
    let rows: Vec<(Option<String>,)> = query.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .map(|(email,)| clean_string(email.as_deref()))
        .collect())
}

pub async fn delete_many(
    pool: &MySqlPool,
    group_uid: &str,
    ids: &[i64],
) -> Result<u64, sqlx::Error> {
    if ids.is_empty() {
        return Ok(0);
    }
    let mut query = QueryBuilder::<MySql>::new("DELETE FROM tkguest WHERE GuestId IN (");
    let mut list = query.separated(", ");
    for id in ids {
        list.push_bind(id);
    }
    query.push(") AND GuestGroupUID = ").push_bind(group_uid);
    Ok(query.build().execute(pool).await?.rows_affected())
}

#[derive(Debug)]
pub struct NewGuest<'a> {
    pub group_uid: &'a str,
    pub creator_email: &'a str,
    pub creator_name: &'a str,
    pub name: &'a str,
    pub destination: &'a str,
    pub guest_type: &'a str,
    pub purpose: Option<&'a str>,
    pub number: Option<&'a str>,
    pub identity: Option<&'a str>,
    pub identity_type: Option<&'a str>,
    pub address: Option<&'a str>,
    pub to_user_email: Option<&'a str>,
    pub photo: Option<&'a str>,
    pub arrived_at: DateTime<Utc>,
}

/// Inserts a guest with a fresh `GuestUID` and QR code and returns its `GuestId`.
pub async fn insert(pool: &MySqlPool, guest: &NewGuest<'_>) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO tkguest (GuestUID, GuestGroupUID, GuestCreatorEmail, GuestCreatorName, \
         GuestName, GuestDestination, GuestPurpose, GuestNumber, GuestIdentity, GuestIdentityType, \
         GuestCategory, GuestType, GuestAddress, GuestToUserEmail, GuestPhoto, GuestDT, GuestAckDT, \
         GuestOutDT, GuestCheckOutByEmail, QRUID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'Umum', ?, ?, ?, ?, ?, NULL, NULL, NULL, ?)",
    )
    .bind(generate_prefixed_uuid_v7("guest"))
    .bind(guest.group_uid)
    .bind(guest.creator_email)
    .bind(guest.creator_name)
    .bind(guest.name)
    .bind(guest.destination)
    .bind(guest.purpose)
    .bind(guest.number)
    .bind(guest.identity)
    .bind(guest.identity_type)
    .bind(guest.guest_type)
    .bind(guest.address)
    .bind(guest.to_user_email)
    .bind(guest.photo)
    .bind(guest.arrived_at)
    .bind(generate_uuid_v4())
    .execute(pool)
    .await?;
    Ok(i64::try_from(result.last_insert_id()).unwrap_or(i64::MAX))
}

/// Columns a PUT/PATCH may change. The first three cannot be cleared; the rest are tri-state.
#[derive(Debug, Default)]
pub struct GuestChanges {
    pub name: Option<String>,
    pub destination: Option<String>,
    pub guest_type: Option<String>,
    pub purpose: Patch<String>,
    pub number: Patch<String>,
    pub identity: Patch<String>,
    pub identity_type: Patch<String>,
    pub address: Patch<String>,
    pub to_user_email: Patch<String>,
    pub photo: Patch<String>,
}

impl GuestChanges {
    /// `(column, new value)` for every column to write; `None` writes `NULL`.
    fn columns(&self) -> Vec<(&'static str, Option<&str>)> {
        let mut columns = Vec::with_capacity(10);
        for (column, value) in [
            ("GuestName", &self.name),
            ("GuestDestination", &self.destination),
            ("GuestType", &self.guest_type),
        ] {
            if let Some(value) = value {
                columns.push((column, Some(value.as_str())));
            }
        }
        for (column, patch) in [
            ("GuestPurpose", &self.purpose),
            ("GuestNumber", &self.number),
            ("GuestIdentity", &self.identity),
            ("GuestIdentityType", &self.identity_type),
            ("GuestAddress", &self.address),
            ("GuestToUserEmail", &self.to_user_email),
            ("GuestPhoto", &self.photo),
        ] {
            match patch {
                Patch::Missing => {}
                Patch::Clear => columns.push((column, None)),
                Patch::Set(value) => columns.push((column, Some(value.as_str()))),
            }
        }
        columns
    }

    pub fn is_empty(&self) -> bool {
        self.columns().is_empty()
    }
}

pub async fn update_fields(
    pool: &MySqlPool,
    id: i64,
    changes: &GuestChanges,
) -> Result<(), sqlx::Error> {
    let columns = changes.columns();
    if columns.is_empty() {
        return Ok(());
    }
    let mut query = QueryBuilder::<MySql>::new("UPDATE tkguest SET ");
    let mut assignments = query.separated(", ");
    for (column, value) in columns {
        assignments
            .push(format!("{column} = "))
            .push_bind_unseparated(value);
    }
    query.push(" WHERE GuestId = ").push_bind(id);
    query.build().execute(pool).await?;
    Ok(())
}

pub async fn mark_acknowledged(pool: &MySqlPool, id: i64) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE tkguest SET GuestAckDT = ? WHERE GuestId = ?")
        .bind(Utc::now())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

pub async fn mark_checked_out(pool: &MySqlPool, id: i64, email: &str) -> Result<(), sqlx::Error> {
    sqlx::query("UPDATE tkguest SET GuestCheckOutByEmail = ?, GuestOutDT = ? WHERE GuestId = ?")
        .bind(email)
        .bind(Utc::now())
        .bind(id)
        .execute(pool)
        .await?;
    Ok(())
}

/// Registered name of the guest type matching `guest_type` by name or UID in the group.
pub async fn canonical_guest_type(
    pool: &MySqlPool,
    group_uid: &str,
    guest_type: &str,
) -> Result<Option<String>, sqlx::Error> {
    let name: Option<(String,)> = sqlx::query_as(
        "SELECT GuestTypeName FROM tkmasterguesttype \
         WHERE GuestTypeGroupUID = ? AND (GuestTypeName = ? OR GuestTypeUID = ?) LIMIT 1",
    )
    .bind(group_uid)
    .bind(guest_type)
    .bind(guest_type)
    .fetch_optional(pool)
    .await?;
    Ok(name.and_then(|(name,)| clean_string(Some(&name))))
}

pub async fn is_joined_member(
    pool: &MySqlPool,
    group_uid: &str,
    email: &str,
) -> Result<bool, sqlx::Error> {
    let found: Option<(i32,)> = sqlx::query_as(
        "SELECT UserGroupId FROM tkusergroup \
         WHERE GroupUID = ? AND Email = ? AND Status = 'JOINED' LIMIT 1",
    )
    .bind(group_uid)
    .bind(email)
    .fetch_optional(pool)
    .await?;
    Ok(found.is_some())
}

pub async fn master_types(
    pool: &MySqlPool,
    group_uid: &str,
) -> Result<Vec<MasterGuestType>, sqlx::Error> {
    let rows: Vec<(i32, String, String)> = sqlx::query_as(
        "SELECT GuestTypeId, GuestTypeUID, GuestTypeName FROM tkmasterguesttype \
         WHERE GuestTypeGroupUID = ? ORDER BY GuestTypeName ASC",
    )
    .bind(group_uid)
    .fetch_all(pool)
    .await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, uid, name)| {
            Some(MasterGuestType {
                id: id.to_string(),
                uid: clean_string(Some(&uid))?,
                name: clean_string(Some(&name))?,
            })
        })
        .collect())
}
