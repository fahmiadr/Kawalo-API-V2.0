//! SQL for `tkguest` and its lookups (`tkgroup`, `tkuser`, `tkusergroup`, `tkmasterguesttype`).

use std::collections::{BTreeSet, HashMap};

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Serialize;
use sqlx::error::BoxDynError;
use sqlx::mysql::{MySqlTypeInfo, MySqlValueRef};
use sqlx::{Decode, FromRow, MySql, MySqlPool, QueryBuilder, Type};

use crate::core::clean::{Patch, clean_integer, clean_naive_datetime, clean_string};
use crate::core::ids::{generate_prefixed_uuid_v7, generate_uuid_v4};
use crate::core::pagination::Pagination;

use super::dto::MasterGuestType;

/// The exact statement Prisma issued, column order included. MariaDB's `filesort` orders rows
/// that share a `GuestDT` second according to the sort-row layout, so any change to this list
/// (an alias, a `CAST`, a subquery) can reorder ties relative to the TypeScript server.
const GUEST_SELECT: &str = "SELECT `tkguest`.`GuestId`, `tkguest`.`GuestUID`, \
    `tkguest`.`GuestGroupUID`, `tkguest`.`GuestName`, `tkguest`.`GuestNumber`, \
    `tkguest`.`GuestIdentity`, `tkguest`.`GuestIdentityType`, `tkguest`.`GuestPhoto`, \
    `tkguest`.`GuestDT`, `tkguest`.`GuestAckDT`, `tkguest`.`GuestOutDT`, \
    `tkguest`.`GuestPurpose`, `tkguest`.`GuestDestination`, `tkguest`.`GuestToUserEmail`, \
    `tkguest`.`GuestCreatorEmail`, `tkguest`.`GuestCreatorName`, `tkguest`.`GuestCategory`, \
    `tkguest`.`QRUID`, `tkguest`.`GuestAddress`, `tkguest`.`GuestType`, \
    `tkguest`.`GuestCheckOutByEmail`, `tkguest`.`GuestToName` FROM `tkguest`";

const SEARCH_COLUMNS: [&str; 15] = [
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
    "GuestToName",
    "GuestCreatorEmail",
    "GuestCreatorName",
    "GuestCheckOutByEmail",
    "QRUID",
];

/// A `TIMESTAMP` that tolerates the legacy zero date `0000-00-00 00:00:00`, which sqlx's own
/// chrono decoding rejects. Zero decodes to `None`.
#[derive(Debug, Clone, Copy)]
struct LegacyTimestamp(Option<NaiveDateTime>);

impl Type<MySql> for LegacyTimestamp {
    fn type_info() -> MySqlTypeInfo {
        <NaiveDateTime as Type<MySql>>::type_info()
    }

    fn compatible(ty: &MySqlTypeInfo) -> bool {
        <DateTime<Utc> as Type<MySql>>::compatible(ty)
    }
}

impl<'r> Decode<'r, MySql> for LegacyTimestamp {
    fn decode(value: MySqlValueRef<'r>) -> Result<Self, BoxDynError> {
        // Binary rows encode a zero date as a bare length prefix of 0, text rows as zeros.
        let raw = <&[u8] as Decode<MySql>>::decode(value.clone())?;
        if raw == [0] || raw.starts_with(b"0000-00-00") {
            return Ok(Self(None));
        }
        NaiveDateTime::decode(value).map(|date| Self(Some(date)))
    }
}

impl LegacyTimestamp {
    /// ISO 8601, `None` for zero and epoch dates.
    fn iso(self) -> Option<String> {
        self.0.and_then(clean_naive_datetime)
    }
}

#[derive(Debug, FromRow)]
struct GuestRow {
    #[sqlx(rename = "GuestId")]
    id: i32,
    #[sqlx(rename = "GuestGroupUID")]
    group_uid: Option<String>,
    #[sqlx(rename = "GuestName")]
    name: Option<String>,
    #[sqlx(rename = "GuestNumber")]
    number: Option<String>,
    #[sqlx(rename = "GuestIdentity")]
    identity: Option<String>,
    #[sqlx(rename = "GuestIdentityType")]
    identity_type: Option<String>,
    #[sqlx(rename = "GuestPhoto")]
    photo: Option<String>,
    #[sqlx(rename = "GuestDT")]
    arrived_at: Option<LegacyTimestamp>,
    #[sqlx(rename = "GuestAckDT")]
    acknowledged_at: Option<LegacyTimestamp>,
    #[sqlx(rename = "GuestOutDT")]
    left_at: Option<LegacyTimestamp>,
    #[sqlx(rename = "GuestPurpose")]
    purpose: Option<String>,
    #[sqlx(rename = "GuestDestination")]
    destination: Option<String>,
    #[sqlx(rename = "GuestToUserEmail")]
    to_user_email: Option<String>,
    #[sqlx(rename = "GuestCreatorEmail")]
    creator_email: Option<String>,
    #[sqlx(rename = "GuestCreatorName")]
    creator_name: Option<String>,
    #[sqlx(rename = "GuestCategory")]
    category: Option<String>,
    #[sqlx(rename = "QRUID")]
    qr_uid: Option<String>,
    #[sqlx(rename = "GuestAddress")]
    address: Option<String>,
    #[sqlx(rename = "GuestType")]
    guest_type: Option<String>,
    #[sqlx(rename = "GuestCheckOutByEmail")]
    checkout_by_email: Option<String>,
    #[sqlx(rename = "GuestToName")]
    to_name: Option<String>,
}

/// A guest as returned by the API, after the legacy-data sanitising of the anti-corruption
/// layer: dirty markers are `null`, emails lower-case, zero and epoch dates dropped.
#[derive(Debug, Clone, Serialize)]
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
    pub guest_to_name: Option<String>,
    pub guest_creator_email: Option<String>,
    pub guest_creator_name: Option<String>,
    pub guest_category: Option<String>,
    pub qr_uid: Option<String>,
    pub guest_address: Option<String>,
    pub guest_type: Option<String>,
    pub guest_checkout_by_email: Option<String>,
    pub guest_checkout_by_name: Option<String>,
}

impl GuestItem {
    /// Numeric `GuestId`; kept as text for the JSON contract.
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

/// `key -> name` for a lookup table, keyed and valued by their cleaned text.
async fn names(
    pool: &MySqlPool,
    sql: &'static str,
    keys: &BTreeSet<String>,
) -> Result<HashMap<String, Option<String>>, sqlx::Error> {
    if keys.is_empty() {
        return Ok(HashMap::new());
    }
    let mut query = QueryBuilder::<MySql>::new(sql);
    let mut list = query.separated(", ");
    for key in keys {
        list.push_bind(key);
    }
    query.push(")");
    let rows: Vec<(String, String)> = query.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(key, name)| Some((clean_string(Some(&key))?, clean_string(Some(&name)))))
        .collect())
}

/// Resolves group and user names for `rows` with one query each, run concurrently.
async fn hydrate(pool: &MySqlPool, rows: Vec<GuestRow>) -> Result<Vec<GuestItem>, sqlx::Error> {
    if rows.is_empty() {
        return Ok(Vec::new());
    }
    let groups: BTreeSet<String> = rows
        .iter()
        .filter_map(|row| clean_string(row.group_uid.as_deref()))
        .collect();
    let emails: BTreeSet<String> = rows
        .iter()
        .flat_map(|row| {
            [
                row.to_user_email.as_deref(),
                row.checkout_by_email.as_deref(),
            ]
        })
        .filter_map(clean_string)
        .collect();
    let (group_names, user_names) = tokio::try_join!(
        names(
            pool,
            "SELECT GroupUID, GroupName FROM tkgroup WHERE GroupUID IN (",
            &groups
        ),
        names(
            pool,
            "SELECT UserEmail, UserName FROM tkuser WHERE UserEmail IN (",
            &emails
        ),
    )?;

    Ok(rows
        .into_iter()
        .map(|row| {
            let group_uid = clean_string(row.group_uid.as_deref());
            let to_user_email = clean_string(row.to_user_email.as_deref());
            let checkout_by_email = clean_string(row.checkout_by_email.as_deref());
            let lookup = |names: &HashMap<String, Option<String>>, key: &Option<String>| {
                key.as_ref()
                    .and_then(|key| names.get(key))
                    .cloned()
                    .flatten()
            };
            GuestItem {
                id: row.id.to_string(),
                group_name: lookup(&group_names, &group_uid),
                guest_name: clean_string(row.name.as_deref()),
                guest_number: clean_string(row.number.as_deref()),
                guest_identity: clean_string(row.identity.as_deref()),
                guest_identity_type: clean_string(row.identity_type.as_deref()),
                guest_photo: clean_string(row.photo.as_deref()),
                guest_dt: row.arrived_at.and_then(LegacyTimestamp::iso),
                guest_ack_dt: row.acknowledged_at.and_then(LegacyTimestamp::iso),
                guest_out_dt: row.left_at.and_then(LegacyTimestamp::iso),
                guest_purpose: clean_string(row.purpose.as_deref()),
                guest_destination: clean_string(row.destination.as_deref()),
                guest_to_user_name: lookup(&user_names, &to_user_email),
                guest_to_user_email: to_user_email,
                guest_to_name: clean_string(row.to_name.as_deref()),
                guest_creator_email: clean_string(row.creator_email.as_deref()),
                guest_creator_name: clean_string(row.creator_name.as_deref()),
                guest_category: clean_string(row.category.as_deref()),
                qr_uid: clean_string(row.qr_uid.as_deref()),
                guest_address: clean_string(row.address.as_deref()),
                guest_type: clean_string(row.guest_type.as_deref()),
                guest_checkout_by_name: lookup(&user_names, &checkout_by_email),
                guest_checkout_by_email: checkout_by_email,
            }
        })
        .collect())
}

pub async fn list(
    pool: &MySqlPool,
    group_uid: &str,
    filter: &GuestFilter,
) -> Result<Vec<GuestItem>, sqlx::Error> {
    let mut query = QueryBuilder::<MySql>::new(GUEST_SELECT);
    query
        .push(" WHERE `tkguest`.`GuestGroupUID` = ")
        .push_bind(group_uid);
    if let Some(start) = filter.start {
        query.push(" AND `tkguest`.`GuestDT` >= ").push_bind(start);
    }
    if let Some(end) = filter.end {
        query.push(" AND `tkguest`.`GuestDT` <= ").push_bind(end);
    }
    let pattern = filter.search.as_ref().map(|search| format!("%{search}%"));
    if let Some(pattern) = &pattern {
        query.push(" AND (");
        for (index, column) in SEARCH_COLUMNS.iter().enumerate() {
            if index > 0 {
                query.push(" OR ");
            }
            query
                .push("`tkguest`.`")
                .push(column)
                .push("` LIKE ")
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
            .push(
                " AND u.UserEmail IN (`tkguest`.`GuestToUserEmail`, \
                 `tkguest`.`GuestCheckOutByEmail`)))",
            );
    }
    if let Some(created_by) = &filter.created_by {
        query
            .push(" AND `tkguest`.`GuestCreatorEmail` = ")
            .push_bind(created_by);
    }
    if let Some(active) = filter.is_active {
        query.push(if active {
            " AND `tkguest`.`GuestOutDT` IS NULL"
        } else {
            " AND `tkguest`.`GuestOutDT` IS NOT NULL"
        });
    }
    query.push(" ORDER BY `tkguest`.`GuestDT` DESC");
    if let Pagination::Page { take, skip } = filter.page {
        query.push(" LIMIT ").push_bind(take);
        query.push(" OFFSET ").push_bind(skip);
    }
    let rows = query.build_query_as::<GuestRow>().fetch_all(pool).await?;
    hydrate(pool, rows).await
}

/// Guest of `group_uid` whose `QRUID` equals `identifier`, or whose numeric id does.
pub async fn find(
    pool: &MySqlPool,
    group_uid: &str,
    identifier: &str,
) -> Result<Option<GuestItem>, sqlx::Error> {
    let mut query = QueryBuilder::<MySql>::new(GUEST_SELECT);
    query
        .push(" WHERE `tkguest`.`GuestGroupUID` = ")
        .push_bind(group_uid)
        .push(" AND (`tkguest`.`QRUID` = ")
        .push_bind(identifier);
    if let Some(id) = clean_integer(Some(identifier)).filter(|&id| id > 0) {
        query.push(" OR `tkguest`.`GuestId` = ").push_bind(id);
    }
    query.push(") LIMIT 1");
    let row = query
        .build_query_as::<GuestRow>()
        .fetch_optional(pool)
        .await?;
    hydrate_one(pool, row).await
}

pub async fn find_by_row_id(pool: &MySqlPool, id: i64) -> Result<Option<GuestItem>, sqlx::Error> {
    let mut query = QueryBuilder::<MySql>::new(GUEST_SELECT);
    query.push(" WHERE `tkguest`.`GuestId` = ").push_bind(id);
    let row = query
        .build_query_as::<GuestRow>()
        .fetch_optional(pool)
        .await?;
    hydrate_one(pool, row).await
}

async fn hydrate_one(
    pool: &MySqlPool,
    row: Option<GuestRow>,
) -> Result<Option<GuestItem>, sqlx::Error> {
    Ok(hydrate(pool, row.into_iter().collect())
        .await?
        .into_iter()
        .next())
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
    pub to_name: Option<&'a str>,
    pub photo: Option<&'a str>,
    pub arrived_at: DateTime<Utc>,
}

/// Inserts a guest with a fresh `GuestUID` and QR code and returns its `GuestId`.
pub async fn insert(pool: &MySqlPool, guest: &NewGuest<'_>) -> Result<i64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO tkguest (GuestUID, GuestGroupUID, GuestCreatorEmail, GuestCreatorName, \
         GuestName, GuestDestination, GuestPurpose, GuestNumber, GuestIdentity, GuestIdentityType, \
         GuestCategory, GuestType, GuestAddress, GuestToUserEmail, GuestToName, GuestPhoto, GuestDT, \
         GuestAckDT, GuestOutDT, GuestCheckOutByEmail, QRUID) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, 'Umum', ?, ?, ?, ?, ?, ?, NULL, NULL, NULL, ?)",
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
    .bind(guest.to_name)
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
    pub to_name: Patch<String>,
    pub photo: Patch<String>,
}

impl GuestChanges {
    /// `(column, new value)` for every column to write; `None` writes `NULL`.
    fn columns(&self) -> Vec<(&'static str, Option<&str>)> {
        let mut columns = Vec::with_capacity(11);
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
            ("GuestToName", &self.to_name),
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
