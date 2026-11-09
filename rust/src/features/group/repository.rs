use std::collections::{HashMap, HashSet};

use sqlx::{MySql, MySqlPool, QueryBuilder};

use super::MemberItem;
use super::collate::locale_compare;
use crate::core::clean::clean_string;

/// Members with status `JOINED`, sorted by display name (email when nameless).
pub async fn members(pool: &MySqlPool, group_uid: &str) -> Result<Vec<MemberItem>, sqlx::Error> {
    let rows: Vec<Option<String>> = sqlx::query_scalar(
        "SELECT Email FROM tkusergroup WHERE GroupUID = ? AND Status = 'JOINED'",
    )
    .bind(group_uid)
    .fetch_all(pool)
    .await?;
    let mut seen = HashSet::new();
    let emails: Vec<String> = rows
        .iter()
        .filter_map(|email| clean_string(email.as_deref()))
        .filter(|email| seen.insert(email.clone()))
        .collect();
    if emails.is_empty() {
        return Ok(Vec::new());
    }

    let mut query = QueryBuilder::<MySql>::new(
        "SELECT UserEmail, UserName, UserProfilePic FROM tkuser WHERE UserEmail IN (",
    );
    let mut list = query.separated(", ");
    for email in &emails {
        list.push_bind(email.as_str());
    }
    query.push(")");
    let users: Vec<(Option<String>, Option<String>, Option<String>)> =
        query.build_query_as().fetch_all(pool).await?;
    let mut profiles: HashMap<String, (Option<String>, Option<String>)> = users
        .into_iter()
        .filter_map(|(email, name, pic)| {
            Some((
                clean_string(email.as_deref())?,
                (clean_string(name.as_deref()), clean_string(pic.as_deref())),
            ))
        })
        .collect();

    let mut members: Vec<MemberItem> = emails
        .into_iter()
        .map(|email| {
            let (name, profile_pic) = profiles.remove(&email).unwrap_or_default();
            MemberItem {
                email,
                name,
                profile_pic,
            }
        })
        .collect();
    members.sort_by(|a, b| {
        locale_compare(
            a.name.as_deref().unwrap_or(&a.email),
            b.name.as_deref().unwrap_or(&b.email),
        )
    });
    Ok(members)
}
