//! Handlers of the guest book; the use cases of the TypeScript version are inlined here.

use axum::extract::State;
use chrono::{DateTime, Utc};

use crate::core::clean::{
    Patch, clean_boolean, clean_date, clean_integer, clean_string, clean_update_string,
};
use crate::core::error::ApiError;
use crate::core::extract::{AuthedJson, CurrentUser, Numeric, ValidatedQuery};
use crate::core::pagination::{Pagination, PaginationParams, resolve_pagination};
use crate::core::response::{ApiResult, created, ok};
use crate::core::state::AppState;

use super::dto::{CreateGuestBody, GuestPath, ListQuery, MasterGuestType, QrBody, UpdateGuestBody};
use super::repository::{self as repo, GuestChanges, GuestFilter, GuestItem, NewGuest};
use crate::features::batch::{DeleteBatchBody, DeleteBatchResult};

/// Roles allowed to write; `acknowledge` is the exception and also admits the invited host.
const STAFF: [&str; 2] = ["ADMIN", "GUARD"];

const MSG_ID_REQUIRED: &str = "ID atau QR tamu wajib diisi.";
const MSG_NOT_FOUND: &str = "Data tamu tidak ditemukan.";
const MSG_INVALID_TYPE: &str = "Tipe tamu tidak valid atau tidak terdaftar pada grup aktif.";
const MSG_INVALID_HOST: &str = "Penghuni yang dituju tidak terdaftar pada grup aktif.";

fn invalid(message: &str) -> ApiError {
    ApiError::validation().with_message(message)
}

fn not_found(message: &str) -> ApiError {
    ApiError::not_found().with_message(message)
}

fn forbidden(message: &str) -> ApiError {
    ApiError::forbidden().with_message(message)
}

/// Read endpoints answer an empty result, not an error, for a token without an active group.
fn group_of(user: &CurrentUser) -> Option<&str> {
    user.group_uid.as_deref().filter(|group| !group.is_empty())
}

fn parse_date(text: &str) -> Option<DateTime<Utc>> {
    let iso = clean_date(Some(text))?;
    DateTime::parse_from_rfc3339(&iso)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

/// The TypeScript server resolves paging twice (request mapper, then data source). `limit=0`
/// yields "no limit" the first time, which the second pass reads as a missing limit: the query
/// really runs with `LIMIT 20 OFFSET 0`. Kept so responses stay identical.
fn double_resolved_page(limit: Option<f64>, offset: Option<f64>) -> Pagination {
    let first = resolve_pagination(PaginationParams { limit, offset });
    resolve_pagination(PaginationParams {
        limit: first.take().map(f64::from),
        offset: first.skip().map(f64::from),
    })
}

/// A QR code in the body wins over the path id.
fn identifier(qr_uid: Option<&str>, path_id: &str) -> Result<String, ApiError> {
    clean_string(qr_uid)
        .or_else(|| clean_string(Some(path_id)))
        .ok_or_else(|| invalid(MSG_ID_REQUIRED))
}

pub async fn master_types(
    State(state): State<AppState>,
    user: CurrentUser,
) -> ApiResult<Vec<MasterGuestType>> {
    let types = match group_of(&user) {
        Some(group) => repo::master_types(&state.db, group).await?,
        None => Vec::new(),
    };
    ok(types, "Daftar tipe tamu master berhasil diambil")
}

pub async fn list(
    State(state): State<AppState>,
    ValidatedQuery(query): ValidatedQuery<ListQuery>,
    user: CurrentUser,
) -> ApiResult<Vec<GuestItem>> {
    let Some(group) = group_of(&user) else {
        return ok(Vec::new(), "Daftar data tamu berhasil diambil");
    };
    let filter = GuestFilter {
        search: clean_string(query.search.as_deref()),
        page: double_resolved_page(
            query.limit.and_then(Numeric::value),
            query.offset.and_then(Numeric::value),
        ),
        start: query.start_date.as_deref().and_then(parse_date),
        end: query.end_date.as_deref().and_then(parse_date),
        created_by: clean_string(query.created_by.as_deref()),
        is_active: clean_boolean(query.is_active.as_deref()),
    };
    ok(
        repo::list(&state.db, group, &filter).await?,
        "Daftar data tamu berhasil diambil",
    )
}

pub async fn get_by_id(
    State(state): State<AppState>,
    GuestPath(id): GuestPath,
    user: CurrentUser,
) -> ApiResult<Option<GuestItem>> {
    let message = "Data tamu berhasil diambil";
    let Some(group) = group_of(&user) else {
        return ok(None, message);
    };
    let id = clean_string(Some(&id)).ok_or_else(|| invalid(MSG_ID_REQUIRED))?;
    let guest = repo::find(&state.db, group, &id)
        .await?
        .ok_or_else(|| not_found(MSG_NOT_FOUND))?;
    ok(Some(guest), message)
}

pub async fn create(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<CreateGuestBody>,
) -> ApiResult<GuestItem> {
    let user = auth.require_auth()?;
    user.require_role(&STAFF)?;
    let group = user.require_group_uid()?;
    let Some(creator_name) = user
        .name
        .as_deref()
        .filter(|name| !name.is_empty() && !user.email.is_empty())
    else {
        return Err(invalid("Data personil (email dan nama) tidak valid."));
    };

    let name =
        clean_string(Some(&body.guest_name)).ok_or_else(|| invalid("Nama tamu wajib diisi."))?;
    let destination = clean_string(Some(&body.guest_destination))
        .ok_or_else(|| invalid("Destinasi tamu wajib diisi."))?;
    let requested_type = clean_string(Some(&body.guest_type))
        .ok_or_else(|| invalid("Tipe tamu wajib diisi dan harus terdaftar pada grup aktif."))?;
    let guest_type = repo::canonical_guest_type(&state.db, group, &requested_type)
        .await?
        .ok_or_else(|| invalid(MSG_INVALID_TYPE))?;
    let to_user_email = clean_string(body.guest_to_user_email.as_deref());
    let to_name = clean_string(body.guest_to_name.as_deref());
    if let Some(email) = &to_user_email
        && !repo::is_joined_member(&state.db, group, email).await?
    {
        return Err(invalid(MSG_INVALID_HOST));
    }

    let purpose = clean_string(body.guest_purpose.as_deref());
    let number = clean_string(body.guest_number.as_deref());
    let identity = clean_string(body.guest_identity.as_deref());
    let identity_type = clean_string(body.guest_identity_type.as_deref());
    let address = clean_string(body.guest_address.as_deref());
    let photo = clean_string(body.guest_photo.as_deref());
    let id = repo::insert(
        &state.db,
        &NewGuest {
            group_uid: group,
            creator_email: &user.email,
            creator_name,
            name: &name,
            destination: &destination,
            guest_type: &guest_type,
            purpose: purpose.as_deref(),
            number: number.as_deref(),
            identity: identity.as_deref(),
            identity_type: identity_type.as_deref(),
            address: address.as_deref(),
            to_user_email: to_user_email.as_deref(),
            to_name: to_name.as_deref(),
            photo: photo.as_deref(),
            arrived_at: body
                .guest_dt
                .as_deref()
                .and_then(parse_date)
                .unwrap_or_else(Utc::now),
        },
    )
    .await?;
    let guest = repo::find_by_row_id(&state.db, id)
        .await?
        .ok_or_else(|| ApiError::internal("Gagal memetakan data tamu yang baru dibuat"))?;
    created(guest, "Data tamu berhasil dibuat")
}

/// PUT and PATCH behave identically: absent fields stay, `null` or blank clears (when the column
/// allows it).
pub async fn update(
    State(state): State<AppState>,
    GuestPath(id): GuestPath,
    AuthedJson(auth, body): AuthedJson<UpdateGuestBody>,
) -> ApiResult<GuestItem> {
    let user = auth.require_auth()?;
    user.require_role(&STAFF)?;
    let group = user.require_group_uid()?;
    let id = clean_string(Some(&id)).ok_or_else(|| invalid("ID tamu wajib diisi."))?;
    let existing = repo::find(&state.db, group, &id)
        .await?
        .ok_or_else(|| not_found(MSG_NOT_FOUND))?;

    if user.role.as_deref() == Some("GUARD")
        && existing.guest_creator_email.as_deref() != Some(user.email.as_str())
    {
        return Err(forbidden(
            "Petugas (Guard) hanya dapat mengubah data tamu miliknya sendiri.",
        ));
    }

    let guest_type = match clean_string(body.guest_type.as_deref()) {
        Some(requested) => Some(
            repo::canonical_guest_type(&state.db, group, &requested)
                .await?
                .ok_or_else(|| invalid(MSG_INVALID_TYPE))?,
        ),
        None => None,
    };
    let to_user_email = clean_update_string(&body.guest_to_user_email);
    if let Patch::Set(email) = &to_user_email
        && !repo::is_joined_member(&state.db, group, email).await?
    {
        return Err(invalid(MSG_INVALID_HOST));
    }

    let changes = GuestChanges {
        name: clean_string(body.guest_name.as_deref()),
        destination: clean_string(body.guest_destination.as_deref()),
        guest_type,
        purpose: clean_update_string(&body.guest_purpose),
        number: clean_update_string(&body.guest_number),
        identity: clean_update_string(&body.guest_identity),
        identity_type: clean_update_string(&body.guest_identity_type),
        address: clean_update_string(&body.guest_address),
        to_user_email,
        to_name: clean_update_string(&body.guest_to_name),
        photo: clean_update_string(&body.guest_photo),
    };
    if changes.is_empty() {
        return ok(existing, "Data tamu berhasil diperbarui");
    }
    let row_id = existing.row_id();
    repo::update_fields(&state.db, row_id, &changes).await?;
    let updated = repo::find_by_row_id(&state.db, row_id)
        .await?
        .ok_or_else(|| ApiError::internal("Gagal memetakan data tamu yang diperbarui"))?;
    ok(updated, "Data tamu berhasil diperbarui")
}

pub async fn acknowledge(
    State(state): State<AppState>,
    GuestPath(id): GuestPath,
    AuthedJson(auth, body): AuthedJson<QrBody>,
) -> ApiResult<GuestItem> {
    let user = auth.require_auth()?;
    let group = user.require_group_uid()?;
    let identifier = identifier(body.qr_uid.as_deref(), &id)?;
    let existing = repo::find(&state.db, group, &identifier)
        .await?
        .ok_or_else(|| not_found(MSG_NOT_FOUND))?;

    if existing.guest_out_dt.is_some() || existing.guest_checkout_by_email.is_some() {
        return Err(invalid(
            "Tamu sudah keluar/selesai kunjungan, tidak dapat dikonfirmasi lagi.",
        ));
    }
    if existing.guest_ack_dt.is_some() {
        return Err(invalid("Kunjungan tamu sudah dikonfirmasi sebelumnya."));
    }
    let is_staff = matches!(user.role.as_deref(), Some("ADMIN" | "GUARD"));
    let is_host = existing.guest_to_user_email.as_deref() == Some(user.email.as_str());
    if !is_staff && !is_host {
        return Err(forbidden(
            "Hanya Admin, Petugas (Guard), atau Penghuni yang dituju yang dapat mengonfirmasi tamu.",
        ));
    }

    let row_id = existing.row_id();
    repo::mark_acknowledged(&state.db, row_id).await?;
    let updated = repo::find_by_row_id(&state.db, row_id)
        .await?
        .ok_or_else(|| ApiError::internal("Gagal memetakan data tamu yang diperbarui"))?;
    ok(updated, "Data tamu berhasil dikonfirmasi")
}

pub async fn checkout(
    State(state): State<AppState>,
    GuestPath(id): GuestPath,
    AuthedJson(auth, body): AuthedJson<QrBody>,
) -> ApiResult<GuestItem> {
    let user = auth.require_auth()?;
    user.require_role(&STAFF)?;
    let group = user.require_group_uid()?;
    let identifier = identifier(body.qr_uid.as_deref(), &id)?;
    let existing = repo::find(&state.db, group, &identifier)
        .await?
        .ok_or_else(|| not_found(MSG_NOT_FOUND))?;

    if existing.guest_out_dt.is_some() || existing.guest_checkout_by_email.is_some() {
        return Err(invalid("Tamu sudah keluar/selesai kunjungan sebelumnya."));
    }

    let row_id = existing.row_id();
    repo::mark_checked_out(&state.db, row_id, &user.email).await?;
    let updated = repo::find_by_row_id(&state.db, row_id)
        .await?
        .ok_or_else(|| ApiError::internal("Gagal memetakan data tamu yang diperbarui"))?;
    ok(updated, "Tamu berhasil check-out")
}

pub async fn delete_one(
    State(state): State<AppState>,
    GuestPath(id): GuestPath,
    user: CurrentUser,
) -> ApiResult<()> {
    user.require_role(&STAFF)?;
    delete_guests(&state, &user, clean_string(Some(&id)).into_iter().collect()).await?;
    ok((), "Data tamu berhasil dihapus")
}

pub async fn delete_batch(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<DeleteBatchBody>,
) -> ApiResult<DeleteBatchResult> {
    let user = auth.require_auth()?;
    user.require_role(&STAFF)?;
    let targets = body
        .ids
        .iter()
        .filter_map(|id| clean_string(Some(id)))
        .collect();
    let deleted_count = delete_guests(&state, &user, targets).await?;
    ok(
        DeleteBatchResult { deleted_count },
        format!("{deleted_count} data tamu berhasil dihapus"),
    )
}

/// Hard-deletes numeric guest ids of the caller's group. Every id must exist in the group, and a
/// GUARD may only delete guests they created.
async fn delete_guests(
    state: &AppState,
    user: &CurrentUser,
    mut targets: Vec<String>,
) -> Result<u64, ApiError> {
    let group = user.require_group_uid()?;
    targets.sort_unstable();
    targets.dedup();
    if targets.is_empty() {
        return Err(invalid("ID tamu wajib diisi."));
    }
    let ids: Vec<i64> = targets
        .iter()
        .filter_map(|id| clean_integer(Some(id)))
        .filter(|&id| id > 0)
        .collect();
    let creators = repo::creators(&state.db, group, &ids).await?;
    if creators.is_empty() {
        return Err(not_found(MSG_NOT_FOUND));
    }
    if creators.len() != targets.len() {
        return Err(not_found(
            "Satu atau lebih data tamu tidak ditemukan dalam grup aktif.",
        ));
    }
    if user.role.as_deref() == Some("GUARD")
        && creators
            .iter()
            .any(|creator| creator.as_deref() != Some(user.email.as_str()))
    {
        return Err(forbidden(
            "Petugas (Guard) hanya dapat menghapus data tamu miliknya sendiri.",
        ));
    }
    Ok(repo::delete_many(&state.db, group, &ids).await?)
}
