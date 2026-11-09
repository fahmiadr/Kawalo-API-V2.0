//! Request and response shapes of `/information`; constraints mirror the TypeBox schemas.

use std::collections::HashMap;

use serde::{Deserialize, Serialize};

use crate::core::clean::Patch;
use crate::core::constants::MAX_PAGE_SIZE;
use crate::core::extract::{Validate, ValidationErrors};

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

/// Read through a map so a repeated parameter keeps its last value, as in Elysia.
/// `limit` and `offset` are `NaN` when they are not numbers, which [`Validate`] rejects.
#[derive(Debug, Deserialize)]
#[serde(from = "HashMap<String, String>")]
pub struct ListQuery {
    pub search: Option<String>,
    pub limit: Option<f64>,
    pub offset: Option<f64>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub created_by: Option<String>,
}

impl From<HashMap<String, String>> for ListQuery {
    fn from(mut params: HashMap<String, String>) -> Self {
        let mut take = |key: &str| params.remove(key);
        let search = take("search");
        let limit = take("limit").map(|raw| js_number(&raw));
        let offset = take("offset").map(|raw| js_number(&raw));
        let start_date = take("start_date");
        let end_date = take("end_date");
        let created_by = take("created_by");
        Self {
            search,
            limit,
            offset,
            start_date,
            end_date,
            created_by,
        }
    }
}

/// `t.Numeric`: JavaScript `Number(text)` for decimal, exponent and `0x`/`0o`/`0b` text; blank or
/// malformed text is `NaN`.
fn js_number(raw: &str) -> f64 {
    let text = raw.trim();
    let radix = match text.get(..2) {
        Some("0x" | "0X") => Some(16),
        Some("0o" | "0O") => Some(8),
        Some("0b" | "0B") => Some(2),
        _ => None,
    };
    match radix {
        // Precision loss above 2^53 matches JavaScript.
        Some(radix) => u64::from_str_radix(&text[2..], radix).map_or(f64::NAN, |n| n as f64),
        None => text.parse::<f64>().unwrap_or(f64::NAN),
    }
}

impl Validate for ListQuery {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.opt_length("search", self.search.as_deref(), 1..);
        errors.opt_range("limit", self.limit, 0.0..=f64::from(MAX_PAGE_SIZE));
        errors.opt_range("offset", self.offset, 0.0..);
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

impl Validate for CreateBody {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.length("title", &self.title, 1..=TITLE_MAX);
        errors.opt_length("text", self.text.as_deref(), ..=TEXT_MAX);
        errors.opt_length("feed_file", self.feed_file.as_deref(), ..=URL_MAX);
        validate_photos(&mut errors, &self.photos);
        errors.opt_length(
            "event_location",
            self.event_location.as_deref(),
            ..=LOCATION_MAX,
        );
        errors.opt_length("glat", self.glat.as_deref(), ..=COORDINATE_MAX);
        errors.opt_length("glong", self.glong.as_deref(), ..=COORDINATE_MAX);
        errors.into_result()
    }
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

impl Validate for UpdateBody {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        patch_length(&mut errors, "title", &self.title, 1..=TITLE_MAX);
        patch_length(&mut errors, "text", &self.text, ..=TEXT_MAX);
        patch_length(&mut errors, "feed_file", &self.feed_file, ..=URL_MAX);
        validate_photos(&mut errors, &self.photos);
        patch_length(
            &mut errors,
            "event_location",
            &self.event_location,
            ..=LOCATION_MAX,
        );
        patch_length(&mut errors, "glat", &self.glat, ..=COORDINATE_MAX);
        patch_length(&mut errors, "glong", &self.glong, ..=COORDINATE_MAX);
        errors.into_result()
    }
}

#[derive(Debug, Deserialize)]
pub struct DeleteBatchBody {
    pub ids: Vec<String>,
}

impl Validate for DeleteBatchBody {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        if self.ids.is_empty() {
            errors.push("ids", "Expected array length to be greater or equal to 1");
        }
        for (index, id) in self.ids.iter().enumerate() {
            check_numeric_id(&mut errors, &format!("ids/{index}"), id);
        }
        errors.into_result()
    }
}

/// `^[0-9]+$`, the pattern of every `FeedId` in a path or body.
pub fn check_numeric_id(errors: &mut ValidationErrors, path: &str, id: &str) {
    if id.is_empty() || !id.bytes().all(|b| b.is_ascii_digit()) {
        errors.push(path, "Expected string to match '^[0-9]+$'");
    }
}

fn patch_length(
    errors: &mut ValidationErrors,
    path: &str,
    value: &Patch<String>,
    bounds: impl std::ops::RangeBounds<usize>,
) {
    if let Patch::Set(text) = value {
        errors.length(path, text, bounds);
    }
}

fn validate_photos(errors: &mut ValidationErrors, photos: &Patch<Vec<String>>) {
    match photos {
        Patch::Missing => {}
        Patch::Clear => errors.push("photos", "Expected array"),
        Patch::Set(items) => {
            if items.len() > MAX_PHOTOS {
                errors.push(
                    "photos",
                    format!("Expected array length to be less or equal to {MAX_PHOTOS}"),
                );
            }
            errors.each_length("photos", items, ..=URL_MAX);
        }
    }
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

#[derive(Debug, Clone, Copy, Serialize)]
pub struct DeleteBatchResult {
    pub deleted_count: u64,
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn numeric_query_text_follows_javascript_number() {
        for (text, expected) in [
            ("5", 5.0),
            ("2.5", 2.5),
            ("1e1", 10.0),
            ("0x5", 5.0),
            ("0b11", 3.0),
            ("0o17", 15.0),
            (" 2 ", 2.0),
            ("-1", -1.0),
        ] {
            assert_eq!(js_number(text), expected, "{text:?}");
        }
        for text in ["", "  ", "abc", "0x", "12abc"] {
            assert!(js_number(text).is_nan(), "{text:?}");
        }
    }

    #[test]
    fn repeated_query_parameters_keep_the_last_value() {
        let params = serde_urlencoded::from_str::<HashMap<String, String>>("limit=1&limit=3&q=x")
            .expect("map");
        assert_eq!(ListQuery::from(params).limit, Some(3.0));
    }
}
