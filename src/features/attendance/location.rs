//! `GET`/`PUT /attendance/location`: the group's single attendance point. Every shift of the
//! group checks in at it; a group without a row falls back to the legacy per-shift location
//! (see `check_repository::effective_location`).

use axum::extract::State;
use serde::{Deserialize, Serialize};

use super::location_repository::{self as repository, Point};
use crate::core::clean::{clean_string, iso_string};
use crate::core::error::ApiError;
use crate::core::extract::schema::{Body, Field, Kind, Schema};
use crate::core::extract::{AuthedJson, CurrentUser};
use crate::core::response::{ApiResult, ok};
use crate::core::state::AppState;
use crate::features::faces::unprocessable;

pub const MIN_RADIUS: i32 = 10;
pub const MAX_RADIUS: i32 = 1000;
const NAME_MAX: usize = 128;
/// Column value for "no text"; legacy rows and the old admin use it everywhere.
const NA: &str = "N/A";

#[derive(Debug, Serialize)]
pub struct Location {
    configured: bool,
    location_name: Option<String>,
    glat: Option<f64>,
    glong: Option<f64>,
    radius: Option<i32>,
    min_radius: i32,
    max_radius: i32,
    updated_by: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Deserialize)]
pub struct LocationBody {
    location_name: Option<String>,
    glat: f64,
    glong: f64,
    radius: f64,
}

impl Body for LocationBody {
    // Radius range and integrality are checked in the handler so the 422 can name the range in
    // Indonesian.
    const SCHEMA: Schema = Schema::new(&[
        Field::nullable("location_name", Kind::text(0, NAME_MAX)),
        Field::required("glat", Kind::number(-90.0, 90.0)),
        Field::required("glong", Kind::number(-180.0, 180.0)),
        Field::required("radius", Kind::number(f64::MIN, f64::MAX)),
    ]);
}

struct Validated {
    name: String,
    lat: f64,
    long: f64,
    radius: i32,
}

impl LocationBody {
    fn validate(self) -> Result<Validated, ApiError> {
        let whole = self.radius.fract() == 0.0
            && (f64::from(MIN_RADIUS)..=f64::from(MAX_RADIUS)).contains(&self.radius);
        if !whole {
            return Err(unprocessable(format!(
                "Radius absen harus bilangan bulat {MIN_RADIUS} sampai {MAX_RADIUS} meter."
            )));
        }
        if self.glat == 0.0 && self.glong == 0.0 {
            return Err(unprocessable("Koordinat titik absen tidak valid."));
        }
        let name = self.location_name.as_deref().map(str::trim).unwrap_or("");
        Ok(Validated {
            name: if name.is_empty() { NA } else { name }.to_owned(),
            lat: self.glat,
            long: self.glong,
            radius: self.radius as i32,
        })
    }
}

async fn current(state: &AppState, group_uid: &str) -> Result<Location, ApiError> {
    let row = repository::find(&state.db, group_uid).await?;
    Ok(Location {
        configured: row.is_some(),
        location_name: row.as_ref().and_then(|r| clean_string(Some(&r.name))),
        glat: row.as_ref().map(|r| r.lat),
        glong: row.as_ref().map(|r| r.long),
        radius: row.as_ref().map(|r| r.radius),
        min_radius: MIN_RADIUS,
        max_radius: MAX_RADIUS,
        updated_by: row.as_ref().map(|r| r.updated_by.clone()),
        updated_at: row.map(|r| iso_string(r.updated_at)),
    })
}

pub async fn get(user: CurrentUser, State(state): State<AppState>) -> ApiResult<Location> {
    let group_uid = user.require_group_uid()?;
    ok(
        current(&state, group_uid).await?,
        "Titik absen berhasil diambil",
    )
}

pub async fn put(
    State(state): State<AppState>,
    AuthedJson(auth, body): AuthedJson<LocationBody>,
) -> ApiResult<Location> {
    // Validated before the role check, like every body-carrying route.
    let wanted = body.validate()?;
    let user = auth.require_role(&["ADMIN", "SUPER_ADMIN"])?;
    let group_uid = user.require_group_uid()?.to_owned();

    let before = current(&state, &group_uid).await?;
    let point = Point {
        name: &wanted.name,
        lat: wanted.lat,
        long: wanted.long,
        radius: wanted.radius,
    };
    repository::upsert(&state.db, &group_uid, &point, &user.email).await?;
    let after = current(&state, &group_uid).await?;
    tracing::info!(
        group = %group_uid,
        actor = %user.email,
        old = ?(before.glat, before.glong, before.radius),
        new = ?(after.glat, after.glong, after.radius),
        "[Attendance] attendance location changed"
    );
    ok(after, "Titik absen berhasil disimpan")
}
