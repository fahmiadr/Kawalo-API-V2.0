//! Request and response shapes of `/shifts`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::ApiError;
use crate::core::extract::schema::{Body, Field, Kind, Schema};
use crate::core::extract::{Validate, ValidationErrors};

const NAME_MAX: usize = 128;
const TEXT_MAX: usize = 128;
const MINUTES_PER_DAY: i32 = 24 * 60;
/// Column value for "no text"; legacy rows and the old admin use it everywhere.
pub const NA: &str = "N/A";

#[derive(Debug, Deserialize)]
pub struct ListQuery {
    pub search: Option<String>,
}

impl Validate for ListQuery {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.opt_length("search", self.search.as_deref(), 1..);
        errors.into_result()
    }
}

/// Body of POST and PUT; PUT replaces the whole shift, so an absent optional field is cleared.
#[derive(Debug, Deserialize)]
pub struct ShiftBody {
    pub name: String,
    pub start_time: String,
    pub end_time: String,
    pub color: String,
    pub description: Option<String>,
}

impl Body for ShiftBody {
    const SCHEMA: Schema = Schema::new(&[
        Field::required("name", Kind::text(1, NAME_MAX)),
        Field::required("start_time", Kind::text(5, 5)),
        Field::required("end_time", Kind::text(5, 5)),
        Field::required("color", Kind::text(1, 16)),
        Field::nullable("description", Kind::text(0, TEXT_MAX)),
    ]);

    // The `HH:MM` format is a schema-level failure (422, before auth) like the length limits.
    fn parse(value: Value) -> Result<Self, String> {
        Self::SCHEMA.check(&value).map_err(|e| e.to_string())?;
        let body: Self = serde_json::from_value(value).map_err(|e| e.to_string())?;
        let mut errors = ValidationErrors::new();
        for (path, raw) in [
            ("start_time", &body.start_time),
            ("end_time", &body.end_time),
        ] {
            if parse_clock(raw).is_none() {
                errors.push(
                    path,
                    format!("Property '{path}' should match format 'HH:MM'"),
                );
            }
        }
        errors.into_result().map_err(|e| e.to_string())?;
        Ok(body)
    }
}

/// A validated shift, in the shape the columns store it.
#[derive(Debug, PartialEq, Eq)]
pub struct ShiftInput {
    pub name: String,
    pub start: (i32, i32),
    pub end: (i32, i32),
    pub color: String,
    pub description: String,
}

fn bad_request(message: &str) -> ApiError {
    ApiError::validation().with_message(message)
}

/// Strict `HH:MM`, 00:00 to 23:59.
fn parse_clock(raw: &str) -> Option<(i32, i32)> {
    let (hours, minutes) = raw.split_once(':')?;
    let digits = |s: &str| s.len() == 2 && s.bytes().all(|b| b.is_ascii_digit());
    if !digits(hours) || !digits(minutes) {
        return None;
    }
    let (hours, minutes) = (hours.parse().ok()?, minutes.parse().ok()?);
    (hours < 24 && minutes < 60).then_some((hours, minutes))
}

fn is_hex_color(raw: &str) -> bool {
    raw.len() == 7 && raw.starts_with('#') && raw[1..].bytes().all(|b| b.is_ascii_hexdigit())
}

fn text_or_na(raw: Option<&str>) -> String {
    raw.map(str::trim)
        .filter(|t| !t.is_empty())
        .unwrap_or(NA)
        .to_owned()
}

impl ShiftBody {
    /// Business rules (400); the body already passed the schema.
    pub fn into_input(self) -> Result<ShiftInput, ApiError> {
        let name = self.name.trim().to_owned();
        if name.is_empty() {
            return Err(bad_request("Nama shift wajib diisi."));
        }
        // The schema already vouched for both clocks.
        let start = parse_clock(&self.start_time).ok_or_else(|| bad_request("Jam tidak valid."))?;
        let end = parse_clock(&self.end_time).ok_or_else(|| bad_request("Jam tidak valid."))?;
        if start == end {
            return Err(bad_request("Jam mulai dan jam selesai tidak boleh sama."));
        }
        if !is_hex_color(&self.color) {
            return Err(bad_request("Warna harus berformat #RRGGBB."));
        }

        Ok(ShiftInput {
            name,
            start,
            end,
            color: self.color.to_uppercase(),
            description: text_or_na(self.description.as_deref()),
        })
    }
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct ShiftItem {
    pub uid: String,
    pub name: String,
    pub start_time: String,
    pub end_time: String,
    pub crosses_midnight: bool,
    pub duration_mins: i32,
    pub color: String,
    pub description: Option<String>,
}

/// `crosses_midnight` and `duration_mins` for a shift from `start` to `end` minutes of the day.
pub fn span(start: i32, end: i32) -> (bool, i32) {
    let minutes = (end - start).rem_euclid(MINUTES_PER_DAY);
    // Legacy rows may have equal clocks; that reads as a full day, never as zero.
    (
        end <= start,
        if minutes == 0 {
            MINUTES_PER_DAY
        } else {
            minutes
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn body(extra: Value) -> Result<ShiftInput, String> {
        let mut base = json!({
            "name": " Shift Pagi ", "start_time": "07:00", "end_time": "15:00", "color": "#3b82f6",
        });
        base.as_object_mut()
            .expect("object")
            .extend(extra.as_object().expect("object").clone());
        ShiftBody::parse(base).and_then(|b| b.into_input().map_err(|e| e.message().to_owned()))
    }

    #[test]
    fn clock_is_strict() {
        for ok in ["00:00", "23:59", "07:05"] {
            assert!(parse_clock(ok).is_some(), "{ok}");
        }
        for bad in ["24:00", "7:00", "07:60", "07-00", "aa:bb", "07:0", "0700"] {
            assert!(parse_clock(bad).is_none(), "{bad}");
        }
    }

    #[test]
    fn schema_rejects_a_bad_clock() {
        let err = body(json!({"start_time": "25:99"})).unwrap_err();
        assert!(err.contains("/start_time"), "{err}");
    }

    #[test]
    fn minimal_shift_defaults() {
        let input = body(json!({})).expect("valid");
        assert_eq!(input.name, "Shift Pagi");
        assert_eq!(input.color, "#3B82F6");
        assert_eq!(input.description, NA);
    }

    #[test]
    fn business_rules() {
        for extra in [
            json!({"start_time": "07:00", "end_time": "07:00"}),
            json!({"color": "biru"}),
        ] {
            assert!(body(extra.clone()).is_err(), "{extra}");
        }
    }

    #[test]
    fn span_handles_midnight_and_full_day() {
        assert_eq!(span(7 * 60, 15 * 60), (false, 480));
        assert_eq!(span(22 * 60, 6 * 60), (true, 480));
        assert_eq!(span(60, 60), (true, 1440));
    }
}
