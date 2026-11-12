use std::collections::{HashMap, HashSet};

use sqlx::{MySql, MySqlPool, QueryBuilder};

use super::MemberItem;
use super::collate::locale_compare;
use crate::core::clean::clean_string;

/// Members with status `JOINED`, sorted by display name (email when nameless).
pub async fn members(pool: &MySqlPool, group_uid: &str) -> Result<Vec<MemberItem>, sqlx::Error> {
    let rows: Vec<(Option<String>, String)> = sqlx::query_as(
        "SELECT Email, Category FROM tkusergroup WHERE GroupUID = ? AND Status = 'JOINED'",
    )
    .bind(group_uid)
    .fetch_all(pool)
    .await?;
    let mut seen = HashSet::new();
    let memberships: Vec<(String, String)> = rows
        .iter()
        .filter_map(|(email, role)| {
            Some((clean_string(email.as_deref())?, role.trim().to_uppercase()))
        })
        .filter(|(email, _)| seen.insert(email.clone()))
        .collect();
    let emails: Vec<&String> = memberships.iter().map(|(email, _)| email).collect();
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

    let mut members: Vec<MemberItem> = memberships
        .into_iter()
        .map(|(email, role)| {
            let (name, profile_pic) = profiles.remove(&email).unwrap_or_default();
            MemberItem {
                email,
                name,
                avatar_url: profile_pic.clone(),
                profile_pic,
                role,
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
