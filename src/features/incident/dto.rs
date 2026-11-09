//! Request schemas (TypeBox limits) and snake_case response items for the Flutter app.

use serde::{Deserialize, Deserializer, Serialize, Serializer};

use super::event::ReplyEntity;
use super::repository::IncidentRow;
use crate::core::clean::iso_string;
use crate::core::constants::MAX_PAGE_SIZE;
use crate::core::extract::{Numeric, Validate, ValidationErrors};

const MAX_PHOTOS: usize = 4;
const MAX_PHOTO_URL_LEN: usize = 2048;

fn validate_photos(errors: &mut ValidationErrors, photos: Option<&[String]>) {
    let Some(photos) = photos else { return };
    if photos.len() > MAX_PHOTOS {
        errors.push(
            "photos",
            format!("Expected array length to be less or equal to {MAX_PHOTOS}"),
        );
    }
    errors.each_length("photos", photos, ..=MAX_PHOTO_URL_LEN);
}

#[derive(Debug, Default, Deserialize)]
pub struct ListQuery {
    pub status: Option<String>,
    pub search: Option<String>,
    pub start_date: Option<String>,
    pub end_date: Option<String>,
    pub limit: Option<Numeric>,
    pub offset: Option<Numeric>,
}

impl Validate for ListQuery {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.opt_length("search", self.search.as_deref(), 1..);
        errors.numeric("limit", self.limit, 0.0..=f64::from(MAX_PAGE_SIZE));
        errors.numeric("offset", self.offset, 0.0..);
        errors.into_result()
    }
}

/// An optional field that may be absent but not `null`, unlike the nullable ones.
fn present<'de, D: Deserializer<'de>, T: Deserialize<'de>>(
    deserializer: D,
) -> Result<Option<T>, D::Error> {
    T::deserialize(deserializer).map(Some)
}

#[derive(Debug, Deserialize)]
pub struct CreateIncident {
    pub title: String,
    // Accepted for the app's sake but never used: the group always comes from the token.
    #[serde(rename = "group_id", default, deserialize_with = "present")]
    pub _group_id: Option<String>,
    pub description: Option<String>,
    pub category: Option<String>,
    pub glat: Option<f64>,
    pub glong: Option<f64>,
    #[serde(default, deserialize_with = "present")]
    pub photos: Option<Vec<String>>,
}

impl Validate for CreateIncident {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.length("title", &self.title, 1..=128);
        errors.opt_length("description", self.description.as_deref(), ..=1024);
        errors.opt_length("category", self.category.as_deref(), ..=16);
        errors.opt_range("glat", self.glat, -90.0..=90.0);
        errors.opt_range("glong", self.glong, -180.0..=180.0);
        validate_photos(&mut errors, self.photos.as_deref());
        errors.into_result()
    }
}

#[derive(Debug, Deserialize)]
pub struct PostReply {
    pub message: String,
    #[serde(default, deserialize_with = "present")]
    pub photos: Option<Vec<String>>,
}

impl Validate for PostReply {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.length("message", &self.message, ..=2000);
        validate_photos(&mut errors, self.photos.as_deref());
        errors.into_result()
    }
}

/// `JSON.stringify` prints whole numbers without a fraction (`5`, not `5.0`).
fn serialize_coordinate<S: Serializer>(
    value: &Option<f64>,
    serializer: S,
) -> Result<S::Ok, S::Error> {
    match *value {
        Some(n) if n.fract() == 0.0 && n.abs() < 1e15 => serializer.serialize_i64(n as i64),
        Some(n) => serializer.serialize_f64(n),
        None => serializer.serialize_none(),
    }
}

#[derive(Debug, Serialize)]
pub struct IncidentItem {
    pub id: String,
    pub group_id: String,
    pub title: String,
    pub description: Option<String>,
    pub category: String,
    #[serde(serialize_with = "serialize_coordinate")]
    pub glat: Option<f64>,
    #[serde(serialize_with = "serialize_coordinate")]
    pub glong: Option<f64>,
    pub photos: Vec<String>,
    pub status: &'static str,
    pub created_by: String,
    pub created_by_name: Option<String>,
    pub resolved_by: Option<String>,
    pub resolved_by_name: Option<String>,
    pub resolved_at: Option<String>,
    /// Deleted incidents are never returned.
    pub deleted_at: (),
    pub created_at: String,
    pub updated_at: String,
}

impl From<IncidentRow> for IncidentItem {
    fn from(row: IncidentRow) -> Self {
        let status = if row.is_resolved() {
            "RESOLVED"
        } else {
            "OPEN"
        };
        Self {
            id: row.id,
            group_id: row.group_uid,
            title: row.title,
            description: row.description,
            category: row.category,
            glat: row.glat,
            glong: row.glong,
            photos: parse_photos(row.photos.as_deref()),
            status,
            created_by: row.created_by,
            created_by_name: row.created_by_name,
            resolved_by: row.resolved_by,
            resolved_by_name: row.resolved_by_name,
            resolved_at: row.resolved_at.map(iso_string),
            deleted_at: (),
            created_at: iso_string(row.created_at),
            updated_at: iso_string(row.updated_at),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct ReplyItem {
    pub id: String,
    pub incident_id: String,
    pub sender_id: String,
    pub sender_name: String,
    pub sender_role: String,
    pub message: String,
    pub photos: Vec<String>,
    pub created_at: String,
}

impl From<&ReplyEntity> for ReplyItem {
    fn from(reply: &ReplyEntity) -> Self {
        Self {
            id: reply.id.clone(),
            incident_id: reply.incident_id.clone(),
            sender_id: reply.sender_email.clone(),
            sender_name: reply.sender_name.clone(),
            sender_role: reply.sender_role.clone(),
            message: reply.message.clone(),
            photos: reply.photos.clone(),
            created_at: iso_string(reply.created_at),
        }
    }
}

#[derive(Debug, Serialize)]
pub struct IncidentDetail {
    pub incident: IncidentItem,
    pub replies: Vec<ReplyItem>,
}

/// The `Photos` column holds a JSON array of URLs; anything else reads as no photos.
pub fn parse_photos(raw: Option<&str>) -> Vec<String> {
    let Some(raw) = raw.filter(|text| !text.is_empty()) else {
        return Vec::new();
    };
    match serde_json::from_str::<serde_json::Value>(raw) {
        Ok(serde_json::Value::Array(items)) => items
            .into_iter()
            .filter_map(|item| match item {
                serde_json::Value::String(url) => Some(url),
                _ => None,
            })
            .collect(),
        _ => Vec::new(),
    }
}

/// `NULL` when there are no photos, a JSON array otherwise.
pub fn photos_column(photos: &[String]) -> Option<String> {
    (!photos.is_empty())
        .then(|| serde_json::to_string(photos).expect("a list of strings is serialisable"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn photos_column_round_trips_and_tolerates_junk() {
        assert_eq!(photos_column(&[]), None);
        let stored = photos_column(&["a.jpg".to_owned(), "b.jpg".to_owned()]);
        assert_eq!(parse_photos(stored.as_deref()), ["a.jpg", "b.jpg"]);
        for junk in [None, Some(""), Some("not json"), Some("{}"), Some("3")] {
            assert!(parse_photos(junk).is_empty(), "{junk:?}");
        }
        assert_eq!(parse_photos(Some(r#"["a", 1, null, "b"]"#)), ["a", "b"]);
    }

    #[test]
    fn whole_coordinates_serialise_like_json_stringify() {
        #[derive(Serialize)]
        struct C(#[serde(serialize_with = "serialize_coordinate")] Option<f64>);
        let text = |v| serde_json::to_string(&C(v)).expect("json");
        assert_eq!(text(Some(5.0)), "5");
        assert_eq!(text(Some(-6.2)), "-6.2");
        assert_eq!(text(Some(-0.0)), "0");
        assert_eq!(text(None), "null");
    }

    #[test]
    fn create_limits_follow_the_schema() {
        let ok = CreateIncident {
            title: "t".into(),
            _group_id: None,
            description: None,
            category: None,
            glat: Some(90.0),
            glong: Some(-180.0),
            photos: Some(vec!["a".into(); 4]),
        };
        assert!(ok.validate().is_ok());
        let bad = CreateIncident {
            title: String::new(),
            glat: Some(91.0),
            photos: Some(vec!["x".repeat(2049); 5]),
            ..ok
        };
        let text = bad.validate().expect_err("invalid").to_string();
        assert!(text.starts_with("/title: Expected string length greater or equal to 1, /glat:"));
        assert!(text.contains("/photos: Expected array length to be less or equal to 4"));
        assert!(text.contains("/photos/4: Expected string length less or equal to 2048"));
    }
}
