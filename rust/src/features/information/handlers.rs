//! Handlers of `/information`, including the rules the TypeScript use cases enforced.

use std::collections::HashSet;

use axum::extract::rejection::PathRejection;
use axum::extract::{Path, State};
use chrono::{DateTime, NaiveDateTime, Utc};

use crate::core::clean::{
    Patch, clean_coordinate, clean_date, clean_string, clean_update_coordinate, clean_update_string,
};
use crate::core::error::ApiError;
use crate::core::extract::{
    AuthContext, AuthedJson, CurrentUser, ValidatedQuery, ValidationErrors,
};
use crate::core::ids::generate_prefixed_uuid_v7;
use crate::core::pagination::{PaginationParams, resolve_pagination};
use crate::core::response::{ApiResult, created, ok};
use crate::core::state::AppState;
use crate::features::information::dto::{
    CreateBody, DeleteBatchBody, DeleteBatchResult, FeedType, InformationItem, ListQuery,
    MAX_PHOTOS, UpdateBody, check_numeric_id,
};
use crate::features::information::repository::{self, FeedUpdate, ListFilter, NewFeed};

/// Roles allowed to write; plain users may only read.
const WRITER_ROLES: [&str; 2] = ["ADMIN", "GUARD"];

const MSG_NOT_FOUND: &str = "Data informasi tidak ditemukan.";

fn not_found(message: &str) -> ApiError {
    ApiError::not_found().with_message(message)
}

fn bad_request(message: &str) -> ApiError {
    ApiError::validation().with_message(message)
}

/// Validates `:id` against `^[0-9]+$`. Elysia checks params before authentication, so this runs
/// first in every handler.
fn path_id(id: Result<Path<String>, PathRejection>) -> Result<String, ApiError> {
    let id = id
        .map(|Path(id)| id)
        .map_err(|rejection| ApiError::invalid_input(rejection.body_text()))?;
    let mut errors = ValidationErrors::new();
    check_numeric_id(&mut errors, "id", &id);
    errors
        .into_result()
        .map_err(|errors| ApiError::invalid_input(errors.to_string()))?;
    Ok(id)
}

/// Positive `FeedId` that fits the `INTEGER` column; anything else cannot exist.
fn feed_id(id: &str) -> Option<i32> {
    id.parse::<i32>().ok().filter(|&n| n > 0)
}

fn iso_to_naive(iso: &str) -> Option<NaiveDateTime> {
    DateTime::parse_from_rfc3339(iso)
        .ok()
        .map(|date| date.naive_utc())
}

/// Cleans photo URLs and keeps the first four non-empty ones, so the filled slots are contiguous.
fn slide_photos(photos: &[String]) -> Vec<String> {
    photos
        .iter()
        .filter_map(|photo| clean_string(Some(photo)))
        .take(MAX_PHOTOS)
        .collect()
}

/// Guards may only touch their own rows; rows without a usable owner are nobody's.
fn ensure_owner(user: &CurrentUser, owner: Option<&str>, message: &str) -> Result<(), ApiError> {
    let is_guard = user.role.as_deref() == Some("GUARD");
    if is_guard && owner != Some(user.email.as_str()) {
        return Err(ApiError::forbidden().with_message(message));
    }
    Ok(())
}

pub async fn list(
    ValidatedQuery(query): ValidatedQuery<ListQuery>,
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Vec<InformationItem>> {
    const MESSAGE: &str = "Daftar data informasi berhasil diambil";
    let Some(group_uid) = user.group_uid.as_deref().filter(|g| !g.is_empty()) else {
        return ok(Vec::new(), MESSAGE);
    };
    let page = resolve_pagination(PaginationParams {
        limit: query.limit,
        offset: query.offset,
    });
    let filter = ListFilter {
        search: clean_string(query.search.as_deref()),
        start: clean_date(query.start_date.as_deref()).and_then(|d| iso_to_naive(&d)),
        end: clean_date(query.end_date.as_deref()).and_then(|d| iso_to_naive(&d)),
        created_by: clean_string(query.created_by.as_deref()),
    };
    let items = repository::list(&state.db, group_uid, &filter, page).await?;
    ok(items, MESSAGE)
}

pub async fn create(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<CreateBody>,
) -> ApiResult<InformationItem> {
    let user = auth.require_auth()?;
    user.require_role(&WRITER_ROLES)?;
    let group_uid = user.require_group_uid()?;
    let name = user.name.as_deref().filter(|n| !n.is_empty());
    let (false, Some(name)) = (user.email.is_empty(), name) else {
        return Err(bad_request("Data personil (email dan nama) tidak valid."));
    };
    let title = clean_string(Some(&body.title))
        .ok_or_else(|| bad_request("Judul informasi wajib diisi."))?;

    let photos = match &body.photos {
        Patch::Set(photos) => slide_photos(photos),
        Patch::Missing | Patch::Clear => Vec::new(),
    };
    let text = clean_string(body.text.as_deref());
    let file = clean_string(body.feed_file.as_deref());
    let event_location = clean_string(body.event_location.as_deref());
    let glat = clean_coordinate(body.glat.as_deref());
    let glong = clean_coordinate(body.glong.as_deref());

    let (group_name, profile_pic) =
        repository::creator_context(&state.db, group_uid, &user.email).await?;
    let uid = generate_prefixed_uuid_v7("feed");
    let new_feed = NewFeed {
        uid: &uid,
        group_uid,
        by_email: &user.email,
        by_name: name,
        group_name: group_name.as_deref(),
        by_profile_pic: profile_pic.as_deref(),
        dt: Utc::now().naive_utc(),
        title: &title,
        text: text.as_deref(),
        feed_type: body.feed_type.unwrap_or(FeedType::Other),
        file: file.as_deref(),
        pics: std::array::from_fn(|i| photos.get(i).map(String::as_str)),
        event_location: event_location.as_deref(),
        glat: glat.as_deref(),
        glong: glong.as_deref(),
    };
    let id = repository::insert(&state.db, &new_feed).await?;
    let item = match i32::try_from(id) {
        Ok(id) => repository::find(&state.db, id, group_uid).await?,
        Err(_) => None,
    };
    let item =
        item.ok_or_else(|| ApiError::internal("Gagal memetakan data informasi yang baru dibuat"))?;
    created(item, "Data informasi berhasil dibuat")
}

/// PUT and PATCH behave the same: only the fields present in the body change.
pub async fn update(
    State(state): State<AppState>,
    id: Result<Path<String>, PathRejection>,
    AuthedJson(auth, body): AuthedJson<UpdateBody>,
) -> ApiResult<InformationItem> {
    let id = path_id(id)?;
    let user = auth.require_auth()?;
    user.require_role(&WRITER_ROLES)?;
    let group_uid = user.require_group_uid()?;

    let feed_id = feed_id(&id).ok_or_else(|| not_found(MSG_NOT_FOUND))?;
    let existing = repository::find(&state.db, feed_id, group_uid)
        .await?
        .ok_or_else(|| not_found(MSG_NOT_FOUND))?;
    ensure_owner(
        &user,
        existing.user_email.as_deref(),
        "Petugas (Guard) hanya dapat mengubah informasi miliknya sendiri.",
    )?;

    // A `null` or blank title or a `null` feed_type means "no change", not "clear".
    let update = FeedUpdate {
        title: match &body.title {
            Patch::Set(title) => clean_string(Some(title)),
            Patch::Missing | Patch::Clear => None,
        },
        text: clean_update_string(&body.text),
        feed_type: match body.feed_type {
            Patch::Set(feed_type) => Some(feed_type),
            Patch::Missing | Patch::Clear => None,
        },
        file: clean_update_string(&body.feed_file),
        photos: match &body.photos {
            Patch::Set(photos) => Some(slide_photos(photos)),
            Patch::Missing | Patch::Clear => None,
        },
        event_location: clean_update_string(&body.event_location),
        glat: clean_update_coordinate(&body.glat),
        glong: clean_update_coordinate(&body.glong),
    };
    if update.is_empty() {
        return ok(existing, "Data informasi berhasil diperbarui");
    }
    repository::update(&state.db, feed_id, group_uid, update).await?;
    let updated = repository::find(&state.db, feed_id, group_uid)
        .await?
        .ok_or_else(|| ApiError::internal("Gagal memetakan data informasi yang diperbarui"))?;
    ok(updated, "Data informasi berhasil diperbarui")
}

pub async fn delete_one(
    State(state): State<AppState>,
    id: Result<Path<String>, PathRejection>,
    AuthContext(auth): AuthContext,
) -> ApiResult<()> {
    let id = path_id(id)?;
    let user = auth.require_auth()?;
    user.require_role(&WRITER_ROLES)?;
    delete_ids(&state, &user, vec![id]).await?;
    ok((), "Data informasi berhasil dihapus")
}

pub async fn delete_batch(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<DeleteBatchBody>,
) -> ApiResult<DeleteBatchResult> {
    let user = auth.require_auth()?;
    user.require_role(&WRITER_ROLES)?;
    let deleted_count = delete_ids(&state, &user, body.ids).await?;
    ok(
        DeleteBatchResult { deleted_count },
        format!("{deleted_count} data informasi berhasil dihapus"),
    )
}

/// All-or-nothing: every distinct id must exist in the active group (and belong to the caller
/// when the caller is a guard) before anything is deleted. Distinctness is by text, so `1` and
/// `01` count as two ids for one row and are reported as missing.
async fn delete_ids(
    state: &AppState,
    user: &CurrentUser,
    ids: Vec<String>,
) -> Result<u64, ApiError> {
    let group_uid = user.require_group_uid()?;
    let mut seen = HashSet::new();
    let targets: Vec<String> = ids
        .into_iter()
        .filter_map(|id| clean_string(Some(&id)))
        .filter(|id| seen.insert(id.clone()))
        .collect();
    if targets.is_empty() {
        return Err(bad_request("ID informasi wajib diisi."));
    }

    let feed_ids: Vec<i32> = targets.iter().filter_map(|id| feed_id(id)).collect();
    let owners = if feed_ids.is_empty() {
        Vec::new()
    } else {
        repository::owners(&state.db, &feed_ids, group_uid).await?
    };
    if owners.is_empty() {
        return Err(not_found(MSG_NOT_FOUND));
    }
    if owners.len() != targets.len() {
        return Err(not_found(
            "Satu atau lebih data informasi tidak ditemukan dalam grup aktif.",
        ));
    }
    for owner in &owners {
        ensure_owner(
            user,
            owner.as_deref(),
            "Petugas (Guard) hanya dapat menghapus informasi miliknya sendiri.",
        )?;
    }
    Ok(repository::delete_many(&state.db, &feed_ids, group_uid).await?)
}
