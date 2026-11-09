//! SQL over `tkfeed` and the legacy-data clean-up applied to its rows.

use std::collections::{HashMap, HashSet};

use chrono::NaiveDateTime;
use sqlx::query_builder::Separated;
use sqlx::{Encode, FromRow, MySql, MySqlPool, QueryBuilder, Type};

use crate::core::clean::{Patch, clean_coordinate, clean_naive_datetime, clean_string};
use crate::core::pagination::Pagination;
use crate::features::information::dto::{FeedType, InformationItem};

/// Date columns are read as text because `TIMESTAMP` columns of legacy rows can hold
/// `0000-00-00 00:00:00`, which the typed decoder rejects.
const SELECT_FEED: &str = "SELECT FeedId, FeedTitle, FeedText, FeedByEmail, FeedByName, \
    CAST(FeedDT AS CHAR) AS FeedDTText, FeedFile, FeedPic, FeedPic2, FeedPic3, FeedPic4, \
    FeedType, FeedByProfilePic, FeedGroupUID, FeedGroupName, FeedEventLocation, \
    CAST(FeedEventStartDT AS CHAR) AS FeedEventStartDTText, \
    CAST(FeedEventEndDT AS CHAR) AS FeedEventEndDTText, FeedEventDuration, GLat, GLong \
    FROM tkfeed";

const DATETIME_FORMAT: &str = "%Y-%m-%d %H:%M:%S";

#[derive(Debug, FromRow)]
struct FeedRow {
    #[sqlx(rename = "FeedId")]
    id: i32,
    #[sqlx(rename = "FeedTitle")]
    title: String,
    #[sqlx(rename = "FeedText")]
    text: Option<String>,
    #[sqlx(rename = "FeedByEmail")]
    by_email: String,
    #[sqlx(rename = "FeedByName")]
    by_name: Option<String>,
    #[sqlx(rename = "FeedDTText")]
    dt: String,
    #[sqlx(rename = "FeedFile")]
    file: Option<String>,
    #[sqlx(rename = "FeedPic")]
    pic: Option<String>,
    #[sqlx(rename = "FeedPic2")]
    pic2: Option<String>,
    #[sqlx(rename = "FeedPic3")]
    pic3: Option<String>,
    #[sqlx(rename = "FeedPic4")]
    pic4: Option<String>,
    #[sqlx(rename = "FeedType")]
    feed_type: Option<String>,
    #[sqlx(rename = "FeedByProfilePic")]
    by_profile_pic: Option<String>,
    #[sqlx(rename = "FeedGroupUID")]
    group_uid: String,
    #[sqlx(rename = "FeedGroupName")]
    group_name: Option<String>,
    #[sqlx(rename = "FeedEventLocation")]
    event_location: Option<String>,
    #[sqlx(rename = "FeedEventStartDTText")]
    event_start_dt: Option<String>,
    #[sqlx(rename = "FeedEventEndDTText")]
    event_end_dt: Option<String>,
    #[sqlx(rename = "FeedEventDuration")]
    event_duration: Option<i32>,
    #[sqlx(rename = "GLat")]
    glat: Option<String>,
    #[sqlx(rename = "GLong")]
    glong: Option<String>,
}

/// A mapped row plus the group UID needed to look up a missing group name.
struct Draft {
    item: InformationItem,
    group_uid: Option<String>,
}

fn clean_db_datetime(text: Option<&str>) -> Option<String> {
    let parsed = NaiveDateTime::parse_from_str(text?, DATETIME_FORMAT).ok()?;
    clean_naive_datetime(parsed)
}

impl From<FeedRow> for Draft {
    fn from(row: FeedRow) -> Self {
        let photos = [&row.pic, &row.pic2, &row.pic3, &row.pic4]
            .into_iter()
            .filter_map(|pic| clean_string(pic.as_deref()))
            .collect();
        let item = InformationItem {
            id: row.id.to_string(),
            title: clean_string(Some(&row.title)),
            text: clean_string(row.text.as_deref()),
            user_email: clean_string(Some(&row.by_email)),
            user_name: clean_string(row.by_name.as_deref()),
            user_profile_pic: clean_string(row.by_profile_pic.as_deref()),
            feed_dt: clean_db_datetime(Some(&row.dt)),
            feed_file: clean_string(row.file.as_deref()),
            photos,
            feed_type: clean_string(row.feed_type.as_deref()),
            group_name: clean_string(row.group_name.as_deref()),
            event_location: clean_string(row.event_location.as_deref()),
            event_start_dt: clean_db_datetime(row.event_start_dt.as_deref()),
            event_end_dt: clean_db_datetime(row.event_end_dt.as_deref()),
            event_duration: row.event_duration,
            glat: clean_coordinate(row.glat.as_deref()),
            glong: clean_coordinate(row.glong.as_deref()),
        };
        Self {
            item,
            group_uid: clean_string(Some(&row.group_uid)),
        }
    }
}

type UserFallback = (Option<String>, Option<String>);

fn push_in<T>(builder: &mut QueryBuilder<MySql>, values: &[T])
where
    for<'t> &'t T: Encode<'t, MySql> + Type<MySql>,
{
    let mut list = builder.push("(").separated(", ");
    for value in values {
        list.push_bind(value);
    }
    list.push_unseparated(")");
}

/// Names and pictures the row itself lacks come from `tkuser` / `tkgroup`. Both lookups are
/// skipped when every row is already complete.
async fn enrich(db: &MySqlPool, rows: Vec<FeedRow>) -> Result<Vec<InformationItem>, sqlx::Error> {
    let mut drafts: Vec<Draft> = rows.into_iter().map(Draft::from).collect();

    let emails = distinct(drafts.iter().filter_map(|d| {
        let incomplete = d.item.user_name.is_none() || d.item.user_profile_pic.is_none();
        d.item.user_email.as_deref().filter(|_| incomplete)
    }));
    let group_uids = distinct(drafts.iter().filter_map(|d| {
        d.group_uid
            .as_deref()
            .filter(|_| d.item.group_name.is_none())
    }));
    if emails.is_empty() && group_uids.is_empty() {
        return Ok(drafts.into_iter().map(|d| d.item).collect());
    }

    let (users, groups) =
        tokio::try_join!(lookup_users(db, &emails), lookup_groups(db, &group_uids))?;
    for draft in &mut drafts {
        let item = &mut draft.item;
        if let Some((name, pic)) = item.user_email.as_deref().and_then(|e| users.get(e)) {
            item.user_name = item.user_name.take().or_else(|| name.clone());
            item.user_profile_pic = item.user_profile_pic.take().or_else(|| pic.clone());
        }
        if item.group_name.is_none() {
            item.group_name = draft
                .group_uid
                .as_deref()
                .and_then(|g| groups.get(g))
                .cloned();
        }
    }
    Ok(drafts.into_iter().map(|d| d.item).collect())
}

fn distinct<'a>(values: impl Iterator<Item = &'a str>) -> Vec<String> {
    let unique: HashSet<&str> = values.collect();
    unique.into_iter().map(str::to_owned).collect()
}

async fn lookup_users(
    db: &MySqlPool,
    emails: &[String],
) -> Result<HashMap<String, UserFallback>, sqlx::Error> {
    if emails.is_empty() {
        return Ok(HashMap::new());
    }
    let mut builder = QueryBuilder::new(
        "SELECT UserEmail, UserName, UserProfilePic FROM tkuser WHERE UserEmail IN ",
    );
    push_in(&mut builder, emails);
    let rows: Vec<(String, String, Option<String>)> =
        builder.build_query_as().fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(email, name, pic)| {
            let email = clean_string(Some(&email))?;
            Some((
                email,
                (clean_string(Some(&name)), clean_string(pic.as_deref())),
            ))
        })
        .collect())
}

async fn lookup_groups(
    db: &MySqlPool,
    group_uids: &[String],
) -> Result<HashMap<String, String>, sqlx::Error> {
    if group_uids.is_empty() {
        return Ok(HashMap::new());
    }
    let mut builder =
        QueryBuilder::new("SELECT GroupUID, GroupName FROM tkgroup WHERE GroupUID IN ");
    push_in(&mut builder, group_uids);
    let rows: Vec<(String, String)> = builder.build_query_as().fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(uid, name)| Some((clean_string(Some(&uid))?, clean_string(Some(&name))?)))
        .collect())
}

#[derive(Debug, Default)]
pub struct ListFilter {
    pub search: Option<String>,
    pub start: Option<NaiveDateTime>,
    pub end: Option<NaiveDateTime>,
    pub created_by: Option<String>,
}

/// Newest first. Rows of the same second keep the order MariaDB returns them in, as with Prisma,
/// so the SQL must stay identical to the TypeScript query.
pub async fn list(
    db: &MySqlPool,
    group_uid: &str,
    filter: &ListFilter,
    page: Pagination,
) -> Result<Vec<InformationItem>, sqlx::Error> {
    let mut builder = QueryBuilder::new(SELECT_FEED);
    builder.push(" WHERE FeedGroupUID = ").push_bind(group_uid);
    if let Some(start) = filter.start {
        builder.push(" AND FeedDT >= ").push_bind(start);
    }
    if let Some(end) = filter.end {
        builder.push(" AND FeedDT <= ").push_bind(end);
    }
    let pattern = filter.search.as_deref().map(|s| format!("%{s}%"));
    if let Some(pattern) = &pattern {
        builder.push(" AND (");
        let mut any = builder.separated(" OR ");
        for column in [
            "FeedTitle",
            "FeedText",
            "FeedByName",
            "FeedByEmail",
            "FeedType",
            "FeedEventLocation",
        ] {
            any.push(format!("{column} LIKE "));
            any.push_bind_unseparated(pattern);
        }
        builder.push(")");
    }
    if let Some(created_by) = &filter.created_by {
        builder.push(" AND FeedByEmail = ").push_bind(created_by);
    }
    builder.push(" ORDER BY FeedDT DESC");
    if let Pagination::Page { take, skip } = page {
        builder.push(" LIMIT ").push_bind(take);
        builder.push(" OFFSET ").push_bind(skip);
    }
    let rows: Vec<FeedRow> = builder.build_query_as().fetch_all(db).await?;
    enrich(db, rows).await
}

pub async fn find(
    db: &MySqlPool,
    id: i32,
    group_uid: &str,
) -> Result<Option<InformationItem>, sqlx::Error> {
    let mut builder = QueryBuilder::new(SELECT_FEED);
    builder.push(" WHERE FeedId = ").push_bind(id);
    builder.push(" AND FeedGroupUID = ").push_bind(group_uid);
    let row: Option<FeedRow> = builder.build_query_as().fetch_optional(db).await?;
    match row {
        Some(row) => Ok(enrich(db, vec![row]).await?.pop()),
        None => Ok(None),
    }
}

/// Cleaned `FeedByEmail` of every existing row among `ids` in the group.
pub async fn owners(
    db: &MySqlPool,
    ids: &[i32],
    group_uid: &str,
) -> Result<Vec<Option<String>>, sqlx::Error> {
    let mut builder = QueryBuilder::new("SELECT FeedByEmail FROM tkfeed WHERE FeedId IN ");
    push_in(&mut builder, ids);
    builder.push(" AND FeedGroupUID = ").push_bind(group_uid);
    let rows: Vec<(String,)> = builder.build_query_as().fetch_all(db).await?;
    Ok(rows
        .into_iter()
        .map(|(email,)| clean_string(Some(&email)))
        .collect())
}

pub async fn delete_many(db: &MySqlPool, ids: &[i32], group_uid: &str) -> Result<u64, sqlx::Error> {
    let mut builder = QueryBuilder::new("DELETE FROM tkfeed WHERE FeedId IN ");
    push_in(&mut builder, ids);
    builder.push(" AND FeedGroupUID = ").push_bind(group_uid);
    Ok(builder.build().execute(db).await?.rows_affected())
}

/// Group name and creator picture stored on the new row.
pub async fn creator_context(
    db: &MySqlPool,
    group_uid: &str,
    email: &str,
) -> Result<(Option<String>, Option<String>), sqlx::Error> {
    let (group_name, profile_pic): (Option<String>, Option<String>) = sqlx::query_as(
        "SELECT (SELECT GroupName FROM tkgroup WHERE GroupUID = ?), \
         (SELECT UserProfilePic FROM tkuser WHERE UserEmail = ?)",
    )
    .bind(group_uid)
    .bind(email)
    .fetch_one(db)
    .await?;
    Ok((
        clean_string(group_name.as_deref()),
        clean_string(profile_pic.as_deref()),
    ))
}

#[derive(Debug)]
pub struct NewFeed<'a> {
    pub uid: &'a str,
    pub group_uid: &'a str,
    pub by_email: &'a str,
    pub by_name: &'a str,
    pub group_name: Option<&'a str>,
    pub by_profile_pic: Option<&'a str>,
    pub dt: NaiveDateTime,
    pub title: &'a str,
    pub text: Option<&'a str>,
    pub feed_type: FeedType,
    pub file: Option<&'a str>,
    pub pics: [Option<&'a str>; 4],
    pub event_location: Option<&'a str>,
    pub glat: Option<&'a str>,
    pub glong: Option<&'a str>,
}

/// Returns the new `FeedId`.
pub async fn insert(db: &MySqlPool, feed: &NewFeed<'_>) -> Result<u64, sqlx::Error> {
    let result = sqlx::query(
        "INSERT INTO tkfeed (FeedUID, FeedGroupUID, FeedByEmail, FeedByName, FeedGroupName, \
         FeedByProfilePic, FeedDT, FeedTitle, FeedText, FeedType, FeedFile, FeedPic, FeedPic2, \
         FeedPic3, FeedPic4, FeedEventLocation, GLat, GLong) \
         VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
    )
    .bind(feed.uid)
    .bind(feed.group_uid)
    .bind(feed.by_email)
    .bind(feed.by_name)
    .bind(feed.group_name)
    .bind(feed.by_profile_pic)
    .bind(feed.dt)
    .bind(feed.title)
    .bind(feed.text)
    .bind(feed.feed_type.as_str())
    .bind(feed.file)
    .bind(feed.pics[0])
    .bind(feed.pics[1])
    .bind(feed.pics[2])
    .bind(feed.pics[3])
    .bind(feed.event_location)
    .bind(feed.glat)
    .bind(feed.glong)
    .execute(db)
    .await?;
    Ok(result.last_insert_id())
}

/// Columns to change; anything left at its default is not touched.
#[derive(Debug, Default)]
pub struct FeedUpdate {
    pub title: Option<String>,
    pub text: Patch<String>,
    pub feed_type: Option<FeedType>,
    pub file: Patch<String>,
    /// All four picture columns are rewritten, missing slots become `NULL`.
    pub photos: Option<Vec<String>>,
    pub event_location: Patch<String>,
    pub glat: Patch<String>,
    pub glong: Patch<String>,
}

impl FeedUpdate {
    pub fn is_empty(&self) -> bool {
        self.title.is_none()
            && self.text.is_missing()
            && self.feed_type.is_none()
            && self.file.is_missing()
            && self.photos.is_none()
            && self.event_location.is_missing()
            && self.glat.is_missing()
            && self.glong.is_missing()
    }
}

fn push_set<'t, T>(set: &mut Separated<'_, MySql, &'static str>, column: &str, value: T)
where
    T: Encode<'t, MySql> + Type<MySql>,
{
    set.push(format!("{column} = "));
    set.push_bind_unseparated(value);
}

fn push_patch(set: &mut Separated<'_, MySql, &'static str>, column: &str, value: Patch<String>) {
    if let Some(value) = value.into_option() {
        push_set(set, column, value);
    }
}

/// Applies `update` to a row of the group. A no-op update issues no statement.
pub async fn update(
    db: &MySqlPool,
    id: i32,
    group_uid: &str,
    update: FeedUpdate,
) -> Result<(), sqlx::Error> {
    if update.is_empty() {
        return Ok(());
    }
    let mut builder = QueryBuilder::new("UPDATE tkfeed SET ");
    let mut set = builder.separated(", ");
    if let Some(title) = update.title {
        push_set(&mut set, "FeedTitle", title);
    }
    push_patch(&mut set, "FeedText", update.text);
    if let Some(feed_type) = update.feed_type {
        push_set(&mut set, "FeedType", feed_type.as_str());
    }
    push_patch(&mut set, "FeedFile", update.file);
    if let Some(photos) = update.photos {
        let mut slots = photos.into_iter();
        for column in ["FeedPic", "FeedPic2", "FeedPic3", "FeedPic4"] {
            push_set(&mut set, column, slots.next());
        }
    }
    push_patch(&mut set, "FeedEventLocation", update.event_location);
    push_patch(&mut set, "GLat", update.glat);
    push_patch(&mut set, "GLong", update.glong);
    builder.push(" WHERE FeedId = ").push_bind(id);
    builder.push(" AND FeedGroupUID = ").push_bind(group_uid);
    builder.build().execute(db).await?;
    Ok(())
}
