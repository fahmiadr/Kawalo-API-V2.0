//! Profile photos of PTT speakers, looked up at read time.
//!
//! A speaker is identified by the token `sub` (the `tkuser.UserId` as text), which Redis and
//! `tkpttrecording` store without a photo: a photo changed after the turn must show up too.

use std::collections::HashMap;

use sqlx::{MySql, MySqlPool, QueryBuilder};

use crate::core::clean::clean_string;

/// `sub` to photo URL, only for ids that parse as a user id and have a photo.
pub async fn by_user_id<'a>(
    pool: &MySqlPool,
    ids: impl IntoIterator<Item = &'a str>,
) -> sqlx::Result<HashMap<String, String>> {
    let numeric: Vec<i32> = ids.into_iter().filter_map(|id| id.parse().ok()).collect();
    if numeric.is_empty() {
        return Ok(HashMap::new());
    }
    let mut query =
        QueryBuilder::<MySql>::new("SELECT UserId, UserProfilePic FROM tkuser WHERE UserId IN (");
    let mut list = query.separated(", ");
    for id in &numeric {
        list.push_bind(*id);
    }
    query.push(")");
    let rows: Vec<(i32, Option<String>)> = query.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(id, pic)| Some((id.to_string(), clean_string(pic.as_deref())?)))
        .collect())
}
