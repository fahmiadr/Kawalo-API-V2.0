//! Route handlers for `/incidents`, ported from the use cases and `IncidentHandler`.

use std::sync::Arc;

use axum::Extension;
use axum::body::Body;
use axum::extract::{FromRequestParts, Path, State};
use axum::http::request::Parts;
use axum::http::{HeaderMap, HeaderValue, StatusCode, header};
use axum::response::Response;
use chrono::{DateTime, SubsecRound, Utc};

use super::bus::{self, IncidentBus};
use super::dto::{
    CreateIncident, IncidentDetail, IncidentItem, ListQuery, PostReply, ReplyItem, photos_column,
};
use super::event::{IncidentEvent, ReplyEntity};
use super::repository::{self, Filter, IncidentRow};
use super::sse;
use crate::core::clean::{clean_date, clean_string};
use crate::core::error::ApiError;
use crate::core::extract::{AuthedJson, CurrentUser, Numeric, ValidatedQuery, ValidationErrors};
use crate::core::ids::generate_prefixed_uuid_v7;
use crate::core::pagination::{PaginationParams, resolve_pagination};
use crate::core::response::{ApiResult, created, ok};
use crate::core::state::AppState;

const CATEGORIES: [&str; 4] = ["facility", "security", "hazard", "general"];
const DEFAULT_CATEGORY: &str = "general";
const DEFAULT_REPLY_ROLE: &str = "USER";
const MSG_NOT_FOUND: &str = "Insiden tidak ditemukan.";

/// The `:id` path segment, validated like `IncidentIdParamsDto` (1 to 64 characters) before auth.
#[derive(Debug)]
pub struct IncidentId(String);

impl<S: Send + Sync> FromRequestParts<S> for IncidentId {
    type Rejection = ApiError;

    async fn from_request_parts(parts: &mut Parts, state: &S) -> Result<Self, Self::Rejection> {
        let Path(id) = Path::<String>::from_request_parts(parts, state)
            .await
            .map_err(|_| ApiError::invalid_input("/id: Expected required property"))?;
        let mut errors = ValidationErrors::new();
        errors.length("id", &id, 1..=64);
        errors
            .into_result()
            .map_err(|errors| ApiError::invalid_input(errors.to_string()))?;
        Ok(Self(id))
    }
}

fn not_found() -> ApiError {
    ApiError::not_found().with_message(MSG_NOT_FOUND)
}

fn invalid(message: &str) -> ApiError {
    ApiError::validation().with_message(message)
}

fn iso_to_utc(iso: String) -> Option<DateTime<Utc>> {
    DateTime::parse_from_rfc3339(&iso)
        .ok()
        .map(|date| date.with_timezone(&Utc))
}

/// Timestamp columns are `TIMESTAMP(0)`; truncating up front keeps responses equal to a re-read.
fn now_seconds() -> DateTime<Utc> {
    Utc::now().trunc_subsecs(0)
}

pub async fn list(
    ValidatedQuery(query): ValidatedQuery<ListQuery>,
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Vec<IncidentItem>> {
    const MESSAGE: &str = "Daftar insiden berhasil diambil";
    let Some(group_uid) = user.group_uid.as_deref().filter(|group| !group.is_empty()) else {
        return ok(Vec::new(), MESSAGE);
    };

    // The query is paged once when mapped and again by the repository. `limit=0` ("no limit")
    // leaves nothing to carry over, so the second pass applies the default page. Kept as is.
    let first = resolve_pagination(PaginationParams {
        limit: query.limit.and_then(Numeric::value),
        offset: query.offset.and_then(Numeric::value),
    });
    let page = resolve_pagination(PaginationParams {
        limit: first.take().map(f64::from),
        offset: first.skip().map(f64::from),
    });
    let status = clean_string(query.status.as_deref()).and_then(|status| {
        match status.to_uppercase().as_str() {
            "OPEN" => Some("OPEN"),
            "RESOLVED" => Some("RESOLVED"),
            _ => None,
        }
    });
    let search = clean_string(query.search.as_deref());
    let filter = Filter {
        status,
        search: search.as_deref(),
        start: clean_date(query.start_date.as_deref()).and_then(iso_to_utc),
        end: clean_date(query.end_date.as_deref()).and_then(iso_to_utc),
        take: page.take().unwrap_or_default(),
        skip: page.skip().unwrap_or_default(),
    };
    let rows = repository::list(&state.db, group_uid, &filter).await?;
    ok(rows.into_iter().map(IncidentItem::from).collect(), MESSAGE)
}

pub async fn detail(
    id: IncidentId,
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<IncidentDetail> {
    let group_uid = user.require_group_uid()?;
    // The replies key is the requested id, which equals the row's id whenever the incident exists,
    // so both reads can run at once.
    let (incident, replies) = tokio::try_join!(
        repository::find(&state.db, &id.0, group_uid),
        repository::replies(&state.db, &id.0),
    )?;
    let incident = incident.ok_or_else(not_found)?;
    ok(
        IncidentDetail {
            replies: replies.iter().map(ReplyItem::from).collect(),
            incident: incident.into(),
        },
        "Detail insiden berhasil diambil",
    )
}

pub async fn create_incident(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<CreateIncident>,
) -> ApiResult<IncidentItem> {
    let user = auth.require_auth()?;
    let group_uid = user.require_group_uid()?;
    let title =
        clean_string(Some(&body.title)).ok_or_else(|| invalid("Judul insiden wajib diisi."))?;

    let category = clean_string(body.category.as_deref()).map_or_else(
        || DEFAULT_CATEGORY.to_owned(),
        |category| category.to_lowercase(),
    );
    if !CATEGORIES.contains(&category.as_str()) {
        return Err(invalid(&format!(
            "Kategori harus salah satu dari: {}.",
            CATEGORIES.join(", ")
        )));
    }
    if body.glat.is_some() != body.glong.is_some() {
        return Err(invalid("Koordinat harus diisi lengkap (glat dan glong)."));
    }

    let photos = clean_photos(body.photos.as_deref());
    let now = now_seconds();
    let row = IncidentRow {
        id: generate_prefixed_uuid_v7("inc"),
        group_uid: group_uid.to_owned(),
        title,
        description: clean_string(body.description.as_deref()),
        category,
        glat: body.glat,
        glong: body.glong,
        photos: photos_column(&photos),
        status: "OPEN".to_owned(),
        created_by_name: Some(user.name.unwrap_or_else(|| user.email.clone())),
        created_by: user.email,
        resolved_by: None,
        resolved_by_name: None,
        resolved_at: None,
        created_at: now,
        updated_at: now,
    };
    repository::insert(&state.db, &row).await?;
    created(row.into(), "Insiden berhasil dilaporkan")
}

pub async fn reply(
    id: IncidentId,
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<PostReply>,
) -> ApiResult<ReplyItem> {
    let user = auth.require_auth()?;
    let group_uid = user.require_group_uid()?;
    let message = clean_string(Some(&body.message)).unwrap_or_default();
    let photos = clean_photos(body.photos.as_deref());
    if message.is_empty() && photos.is_empty() {
        return Err(invalid("Pesan atau foto wajib diisi."));
    }

    let incident = repository::find(&state.db, &id.0, group_uid)
        .await?
        .ok_or_else(not_found)?;
    if incident.is_resolved() {
        return Err(invalid("Insiden sudah diselesaikan, diskusi ditutup."));
    }

    let reply = ReplyEntity {
        id: generate_prefixed_uuid_v7("rpl"),
        incident_id: incident.id.clone(),
        sender_name: user.name.unwrap_or_else(|| user.email.clone()),
        sender_email: user.email,
        sender_role: user.role.unwrap_or_else(|| DEFAULT_REPLY_ROLE.to_owned()),
        message,
        photos,
        created_at: now_seconds(),
    };
    repository::insert_reply(&state.db, &reply).await?;
    let item = ReplyItem::from(&reply);
    let event = IncidentEvent::NewReply { reply };
    bus::publish(&state, &incident.id, &event).await;
    created(item, "Balasan berhasil dikirim")
}

pub async fn resolve(
    id: IncidentId,
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<IncidentItem> {
    let group_uid = user.require_group_uid()?;
    let incident = repository::find(&state.db, &id.0, group_uid)
        .await?
        .ok_or_else(not_found)?;
    const MESSAGE: &str = "Insiden berhasil diselesaikan";
    if incident.is_resolved() {
        // Idempotent: nothing changes, so no duplicate event.
        return ok(incident.into(), MESSAGE);
    }

    let resolved_by_name = user.name.unwrap_or_else(|| user.email.clone());
    let won = repository::resolve(
        &state.db,
        &incident.id,
        &user.email,
        &resolved_by_name,
        now_seconds(),
    )
    .await?;
    let resolved = repository::find_any(&state.db, &incident.id)
        .await?
        .ok_or_else(not_found)?;

    // With two concurrent resolves only the one whose update matched publishes.
    if won && let Some(resolved_at) = resolved.resolved_at {
        let event = IncidentEvent::ThreadResolved {
            incident_id: incident.id.clone(),
            resolved_by: user.email,
            resolved_by_name,
            resolved_at,
        };
        bus::publish(&state, &incident.id, &event).await;
    }
    ok(resolved.into(), MESSAGE)
}

pub async fn remove(
    id: IncidentId,
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<()> {
    let group_uid = user.require_group_uid()?;
    if !user.allow_admin {
        return Err(
            ApiError::forbidden().with_message("Hanya administrator yang dapat menghapus insiden.")
        );
    }
    let incident = repository::find(&state.db, &id.0, group_uid)
        .await?
        .ok_or_else(not_found)?;

    let deleted_at = Utc::now();
    repository::soft_delete(&state.db, &incident.id, deleted_at).await?;
    let event = IncidentEvent::IncidentDeleted {
        incident_id: incident.id.clone(),
        deleted_at,
    };
    bus::publish(&state, &incident.id, &event).await;
    ok((), "Insiden berhasil dihapus")
}

/// Access is checked first, so a failure is a plain JSON error rather than an SSE stream.
pub async fn stream(
    id: IncidentId,
    user: CurrentUser,
    State(state): State<AppState>,
    Extension(bus): Extension<Arc<IncidentBus>>,
    headers: HeaderMap,
) -> Result<Response, ApiError> {
    let group_uid = user.require_group_uid()?;
    let incident = repository::find(&state.db, &id.0, group_uid)
        .await?
        .ok_or_else(not_found)?;

    let reconnect = headers
        .get("last-event-id")
        .is_some_and(|value| !value.is_empty());
    // Subscribing before the response goes out means no event after the headers can be missed.
    let subscription = bus.subscribe(&state, &incident.id).await?;
    let body = Body::from_stream(sse::frames(subscription, reconnect, sse::HEARTBEAT));

    let mut response = Response::new(body);
    *response.status_mut() = StatusCode::OK;
    let headers = response.headers_mut();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_static("text/event-stream;charset=utf-8"),
    );
    headers.insert(
        header::CACHE_CONTROL,
        HeaderValue::from_static("no-cache, no-transform"),
    );
    headers.insert(header::CONNECTION, HeaderValue::from_static("keep-alive"));
    headers.insert("x-accel-buffering", HeaderValue::from_static("no"));
    Ok(response)
}

fn clean_photos(photos: Option<&[String]>) -> Vec<String> {
    photos
        .unwrap_or_default()
        .iter()
        .filter_map(|photo| clean_string(Some(photo)))
        .collect()
}
