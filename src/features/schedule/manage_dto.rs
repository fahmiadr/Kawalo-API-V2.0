use chrono::NaiveDate;
use serde::de::Error as _;
use serde::{Deserialize, Deserializer, Serialize};
use serde_json::Value;

use super::dto::{parse_date, resolve_range};
use crate::core::error::ErrorCode;
use crate::core::extract::schema::{Body, Field, Kind, Schema};
use crate::core::extract::{Validate, ValidationErrors};

pub const MAX_BULK_ITEMS: usize = 1000;

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    from: Option<String>,
    to: Option<String>,
    pub email: Option<String>,
    pub shift_uid: Option<String>,
}

impl ListQuery {
    pub fn resolve(&self, today: NaiveDate) -> Result<(NaiveDate, NaiveDate), ValidationErrors> {
        resolve_range(self.from.as_deref(), self.to.as_deref(), today)
    }
}

impl Validate for ListQuery {
    fn validate(&self) -> Result<(), ValidationErrors> {
        self.resolve(chrono::Local::now().date_naive()).map(drop)
    }
}

fn strict_date<'de, D: Deserializer<'de>>(deserializer: D) -> Result<NaiveDate, D::Error> {
    let raw = String::deserialize(deserializer)?;
    parse_date(&raw).ok_or_else(|| D::Error::custom("'date' harus berformat YYYY-MM-DD"))
}

#[derive(Debug, Deserialize)]
pub struct Entry {
    pub email: String,
    pub shift_uid: String,
    #[serde(deserialize_with = "strict_date")]
    pub date: NaiveDate,
}

const ENTRY: Schema = Schema::new(&[
    Field::required("email", Kind::text(1, 128)),
    Field::required("shift_uid", Kind::text(1, 64)),
    Field::required("date", Kind::text(10, 10)),
]);

/// Schema check plus the strict date format, which the declarative schema cannot express.
fn check_entry(value: &Value) -> Result<(), String> {
    ENTRY.check(value).map_err(|e| e.to_string())?;
    match value.get("date").and_then(Value::as_str) {
        Some(date) if parse_date(date).is_some() => Ok(()),
        _ => Err("/date: 'date' harus berformat YYYY-MM-DD".to_owned()),
    }
}

/// `POST /schedules` and `PUT /schedules/{id}` (which ignores `overwrite`).
#[derive(Debug, Deserialize)]
pub struct ScheduleBody {
    #[serde(flatten)]
    pub entry: Entry,
    #[serde(default)]
    pub overwrite: bool,
}

impl Body for ScheduleBody {
    const SCHEMA: Schema = ENTRY;

    fn parse(value: Value) -> Result<Self, String> {
        check_entry(&value)?;
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Deserialize)]
pub struct BulkBody {
    pub items: Vec<Entry>,
    #[serde(default)]
    pub overwrite: bool,
}

impl Body for BulkBody {
    const SCHEMA: Schema = Schema::new(&[]);

    fn parse(value: Value) -> Result<Self, String> {
        let Some(items) = value.get("items").and_then(Value::as_array) else {
            return Err("/items: Expected array".to_owned());
        };
        if items.is_empty() {
            return Err("/items: Expected array length to be greater or equal to 1".to_owned());
        }
        if items.len() > MAX_BULK_ITEMS {
            return Err(format!(
                "/items: Expected array length to be less or equal to {MAX_BULK_ITEMS}"
            ));
        }
        for (index, item) in items.iter().enumerate() {
            check_entry(item).map_err(|e| format!("/items/{index}{e}"))?;
        }
        serde_json::from_value(value).map_err(|e| e.to_string())
    }
}

#[derive(Debug, Deserialize)]
pub struct DeleteBody {
    pub ids: Vec<String>,
}

impl Body for DeleteBody {
    const SCHEMA: Schema = Schema::new(&[Field::required(
        "ids",
        Kind::list(&Kind::digits(), 1, MAX_BULK_ITEMS),
    )]);
}

#[derive(Debug, Serialize)]
pub struct SkippedItem {
    pub index: usize,
    pub reason: &'static str,
    /// Set only for conflicts the app acts on (other shift that day, so it can offer overwrite).
    #[serde(skip_serializing_if = "Option::is_none")]
    pub code: Option<ErrorCode>,
}

#[derive(Debug, Serialize)]
pub struct BulkResult {
    pub created: usize,
    /// Rows deleted to make room for `created` rows (only with `overwrite`).
    pub replaced: usize,
    pub skipped: usize,
    pub skipped_items: Vec<SkippedItem>,
}

#[derive(Debug, Serialize)]
pub struct DeleteResult {
    pub deleted_count: u64,
    pub skipped_count: u64,
}
