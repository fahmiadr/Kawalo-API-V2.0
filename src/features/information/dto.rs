//! Request and response shapes of `/information`; constraints mirror the TypeBox schemas.

use serde::{Deserialize, Serialize};

use crate::core::clean::Patch;
use crate::core::constants::MAX_PAGE_SIZE;
use crate::core::extract::schema::{Body, Field, Kind, Schema};
use crate::core::extract::{Numeric, Validate, ValidationErrors};

const TITLE_MAX: usize = 128;
const TEXT_MAX: usize = 1024;
const URL_MAX: usize = 2048;
const LOCATION_MAX: usize = 128;
const COORDINATE_MAX: usize = 32;
/// `FeedPic` to `FeedPic4`.
pub const MAX_PHOTOS: usize = 4;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "UPPERCASE")]
pub enum FeedType {
    General,
    Join,
    Incident,
    Event,
    Other,
}

const FEED_TYPES: [&str; 5] = ["GENERAL", "JOIN", "INCIDENT", "EVENT", "OTHER"];
const PHOTOS: Kind = Kind::list(&Kind::text(0, URL_MAX), 0, MAX_PHOTOS);

impl FeedType {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::General => "GENERAL",
            Self::Join => "JOIN",
            Self::Incident => "INCIDENT",
            Self::Event => "EVENT",
            Self::Other => "OTHER",
        }
    }
}

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub search: Option<String>,
    pub limit: Option<Numeric>,
    pub offset: Option<Numeric>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub created_by: Option<String>,
}

impl Validate for ListQuery {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.opt_length("search", self.search.as_deref(), 1..);
        errors.numeric("limit", self.limit, 0.0..=f64::from(MAX_PAGE_SIZE));
        errors.numeric("offset", self.offset, 0.0..);
        errors.opt_length("created_by", self.created_by.as_deref(), 1..);
        errors.into_result()
    }
}

/// `null` is accepted for the scalar fields and rejected for `photos`.
#[derive(Debug, Deserialize)]
pub struct CreateBody {
    pub title: String,
    pub text: Option<String>,
    pub feed_file: Option<String>,
    #[serde(default)]
    pub photos: Patch<Vec<String>>,
    pub feed_type: Option<FeedType>,
    pub event_location: Option<String>,
    pub glat: Option<String>,
    pub glong: Option<String>,
}

impl Body for CreateBody {
    // `feed_type` has a TypeBox default, which makes Elysia treat a missing body as `{}`.
    const SCHEMA: Schema = Schema::new(&[
        Field::required("title", Kind::text(1, TITLE_MAX)),
        Field::nullable("text", Kind::text(0, TEXT_MAX)),
        Field::nullable("feed_file", Kind::text(0, URL_MAX)),
        Field::optional("photos", PHOTOS),
        Field::nullable("feed_type", Kind::OneOf(&FEED_TYPES)),
        Field::nullable("event_location", Kind::text(0, LOCATION_MAX)),
        Field::nullable("glat", Kind::text(0, COORDINATE_MAX)),
        Field::nullable("glong", Kind::text(0, COORDINATE_MAX)),
    ])
    .with_defaults();
}

/// Shared by PUT and PATCH. Every field is tri-state: absent leaves the column alone.
#[derive(Debug, Deserialize)]
pub struct UpdateBody {
    #[serde(default)]
    pub title: Patch<String>,
    #[serde(default)]
    pub text: Patch<String>,
    #[serde(default)]
    pub feed_file: Patch<String>,
    #[serde(default)]
    pub photos: Patch<Vec<String>>,
    #[serde(default)]
    pub feed_type: Patch<FeedType>,
    #[serde(default)]
    pub event_location: Patch<String>,
    #[serde(default)]
    pub glat: Patch<String>,
    #[serde(default)]
    pub glong: Patch<String>,
}

impl Body for UpdateBody {
    const SCHEMA: Schema = Schema::new(&[
        Field::nullable("title", Kind::text(1, TITLE_MAX)),
        Field::nullable("text", Kind::text(0, TEXT_MAX)),
        Field::nullable("feed_file", Kind::text(0, URL_MAX)),
        Field::optional("photos", PHOTOS),
        Field::nullable("feed_type", Kind::OneOf(&FEED_TYPES)),
        Field::nullable("event_location", Kind::text(0, LOCATION_MAX)),
        Field::nullable("glat", Kind::text(0, COORDINATE_MAX)),
        Field::nullable("glong", Kind::text(0, COORDINATE_MAX)),
    ]);
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct InformationItem {
    pub id: String,
    pub title: Option<String>,
    pub text: Option<String>,
    pub user_email: Option<String>,
    pub user_name: Option<String>,
    pub user_profile_pic: Option<String>,
    pub feed_dt: Option<String>,
    pub feed_file: Option<String>,
    pub photos: Vec<String>,
    pub feed_type: Option<String>,
    pub group_name: Option<String>,
    pub event_location: Option<String>,
    pub event_start_dt: Option<String>,
    pub event_end_dt: Option<String>,
    pub event_duration: Option<i32>,
    pub glat: Option<String>,
    pub glong: Option<String>,
}
