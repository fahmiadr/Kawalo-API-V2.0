//! Users and their group memberships, read from `tkuser`, `tkusergroup` and `tkgroup`.

use serde::Serialize;
use sqlx::{FromRow, MySqlPool};

use crate::core::clean::clean_string;

/// Membership status that allows logging in.
pub const JOINED: &str = "JOINED";

/// What a user may do in one group, with the group's own details.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct GroupAccess {
    pub group_uid: String,
    pub group_name: Option<String>,
    pub group_owner: Option<String>,
    pub group_logo: Option<String>,
    pub group_status: Option<String>,
    /// Upper-cased `tkusergroup.Category`; becomes the `role` claim.
    pub category: Option<String>,
    pub status: Option<String>,
    pub allow_admin: bool,
    pub allow_patrol: bool,
    pub allow_mon_patrol: bool,
    pub features: Vec<String>,
}

impl GroupAccess {
    pub fn is_joined(&self) -> bool {
        self.status.as_deref() == Some(JOINED)
    }
}

/// The stored password. The legacy schema keeps it in plain text, so it is compared as is and
/// never printed.
#[derive(Clone)]
pub struct Secret(String);

impl Secret {
    /// Constant-time for equal lengths.
    pub fn matches(&self, candidate: &str) -> bool {
        let (a, b) = (self.0.as_bytes(), candidate.as_bytes());
        a.len() == b.len() && a.iter().zip(b).fold(0u8, |diff, (x, y)| diff | (x ^ y)) == 0
    }
}

impl std::fmt::Debug for Secret {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str("Secret(..)")
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct User {
    pub id: String,
    pub email: String,
    pub name: String,
    #[serde(skip)]
    pub password: Secret,
    pub avatar_url: Option<String>,
    pub active_group: Option<GroupAccess>,
    pub groups: Vec<GroupAccess>,
}

impl User {
    pub fn group(&self, group_uid: &str) -> Option<&GroupAccess> {
        self.groups.iter().find(|g| g.group_uid == group_uid)
    }
}

#[derive(FromRow)]
struct UserRow {
    #[sqlx(rename = "UserId")]
    id: i32,
    #[sqlx(rename = "UserEmail")]
    email: String,
    #[sqlx(rename = "UserName")]
    name: String,
    #[sqlx(rename = "Password")]
    password: String,
    #[sqlx(rename = "UserProfilePic")]
    avatar_url: Option<String>,
}

#[derive(FromRow)]
struct MembershipRow {
    #[sqlx(rename = "GroupUID")]
    group_uid: String,
    #[sqlx(rename = "AllowAdmin")]
    allow_admin: String,
    #[sqlx(rename = "AllowPatrol")]
    allow_patrol: String,
    #[sqlx(rename = "AllowMonPatrol")]
    allow_mon_patrol: String,
    #[sqlx(rename = "Category")]
    category: String,
    #[sqlx(rename = "Status")]
    status: String,
    #[sqlx(rename = "GroupName")]
    group_name: Option<String>,
    #[sqlx(rename = "GroupOwner")]
    group_owner: Option<String>,
    #[sqlx(rename = "GroupLogo")]
    group_logo: Option<String>,
    #[sqlx(rename = "GroupStatus")]
    group_status: Option<String>,
    #[sqlx(rename = "Features")]
    features: Option<String>,
}

impl MembershipRow {
    /// `None` for rows whose group UID is blank or a placeholder.
    fn into_access(self) -> Option<GroupAccess> {
        let clean = |value: &str| clean_string(Some(value));
        let flag = |value: &str| clean(value).is_some_and(|v| v.eq_ignore_ascii_case("YES"));
        let opt = |value: &Option<String>| clean_string(value.as_deref());
        Some(GroupAccess {
            group_uid: clean(&self.group_uid)?,
            group_name: opt(&self.group_name),
            group_owner: opt(&self.group_owner),
            group_logo: opt(&self.group_logo),
            group_status: opt(&self.group_status),
            category: clean(&self.category).map(|c| c.to_uppercase()),
            status: clean(&self.status),
            allow_admin: flag(&self.allow_admin),
            allow_patrol: flag(&self.allow_patrol),
            allow_mon_patrol: flag(&self.allow_mon_patrol),
            features: opt(&self.features)
                .map(|f| f.split(';').filter_map(clean).collect())
                .unwrap_or_default(),
        })
    }
}

/// Finds a user by e-mail (as cleaned by `clean_string`).
///
/// `requested_group` selects the active group; without it the first `JOINED` membership is used.
///
/// # Errors
/// Database errors.
pub async fn find_by_email(
    db: &MySqlPool,
    email: &str,
    requested_group: Option<&str>,
) -> Result<Option<User>, sqlx::Error> {
    let row = sqlx::query_as::<_, UserRow>(
        "SELECT UserId, UserEmail, UserName, Password, UserProfilePic \
         FROM tkuser WHERE UserEmail = ? LIMIT 1",
    )
    .bind(email)
    .fetch_optional(db)
    .await?;
    hydrate(db, row, requested_group).await
}

/// Finds a user by the numeric `UserId` carried in the token `sub`; a non-numeric id finds nothing.
///
/// # Errors
/// Database errors.
pub async fn find_by_id(
    db: &MySqlPool,
    id: &str,
    requested_group: Option<&str>,
) -> Result<Option<User>, sqlx::Error> {
    let Some(id) = id.trim().parse::<i32>().ok().filter(|&n| n > 0) else {
        return Ok(None);
    };
    let row = sqlx::query_as::<_, UserRow>(
        "SELECT UserId, UserEmail, UserName, Password, UserProfilePic \
         FROM tkuser WHERE UserId = ? LIMIT 1",
    )
    .bind(id)
    .fetch_optional(db)
    .await?;
    hydrate(db, row, requested_group).await
}

async fn hydrate(
    db: &MySqlPool,
    row: Option<UserRow>,
    requested_group: Option<&str>,
) -> Result<Option<User>, sqlx::Error> {
    let Some(row) = row else { return Ok(None) };
    let (Some(email), Some(name), Some(password)) = (
        clean_string(Some(&row.email)),
        clean_string(Some(&row.name)),
        clean_string(Some(&row.password)),
    ) else {
        return Ok(None);
    };

    let memberships = sqlx::query_as::<_, MembershipRow>(
        "SELECT ug.GroupUID, ug.AllowAdmin, ug.AllowPatrol, ug.AllowMonPatrol, ug.Category, \
                ug.Status, g.GroupName, g.GroupOwner, g.GroupLogo, g.Status AS GroupStatus, \
                g.Features \
         FROM tkusergroup ug LEFT JOIN tkgroup g ON g.GroupUID = ug.GroupUID \
         WHERE ug.Email = ? ORDER BY ug.UserGroupId",
    )
    .bind(&email)
    .fetch_all(db)
    .await?;
    let groups: Vec<GroupAccess> = memberships
        .into_iter()
        .filter_map(MembershipRow::into_access)
        .collect();

    let active_group = match requested_group.and_then(|g| clean_string(Some(g))) {
        Some(uid) => groups.iter().find(|g| g.group_uid == uid),
        None => groups.iter().find(|g| g.is_joined()),
    }
    .cloned();

    Ok(Some(User {
        id: row.id.to_string(),
        email,
        name,
        password: Secret(password),
        avatar_url: clean_string(row.avatar_url.as_deref()),
        active_group,
        groups,
    }))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn secret_compares_exactly_and_hides_itself() {
        let secret = Secret("rahasia123".to_owned());
        assert!(secret.matches("rahasia123"));
        assert!(!secret.matches("rahasia124"));
        assert!(!secret.matches("rahasia12"));
        assert!(!secret.matches(""));
        assert_eq!(format!("{secret:?}"), "Secret(..)");
    }
}
