//! Request and response shapes of `/shifts`.

use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::core::error::ApiError;
use crate::core::extract::schema::{Body, Field, Kind, Schema};
use crate::core::extract::{Validate, ValidationErrors};

const NAME_MAX: usize = 128;
const TEXT_MAX: usize = 128;
const MINUTES_PER_DAY: i32 = 24 * 60;
pub const RADIUS_MIN: i32 = 10;
pub const RADIUS_MAX: i32 = 1000;
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
    pub location_name: Option<String>,
    pub glat: Option<f64>,
    pub glong: Option<f64>,
    pub radius: Option<f64>,
}

impl Body for ShiftBody {
    const SCHEMA: Schema = Schema::new(&[
        Field::required("name", Kind::text(1, NAME_MAX)),
        Field::required("start_time", Kind::text(5, 5)),
        Field::required("end_time", Kind::text(5, 5)),
        Field::required("color", Kind::text(1, 16)),
        Field::nullable("description", Kind::text(0, TEXT_MAX)),
        Field::nullable("location_name", Kind::text(0, TEXT_MAX)),
        Field::nullable("glat", Kind::number(-90.0, 90.0)),
        Field::nullable("glong", Kind::number(-180.0, 180.0)),
        Field::nullable("radius", Kind::number(0.0, 1_000_000.0)),
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
    pub location_name: String,
    pub location: Option<(String, String)>,
    pub radius: i32,
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

/// At most seven decimals (about a centimetre) without trailing zeros; fits `VARCHAR(32)`.
fn coordinate_text(value: f64) -> String {
    let text = format!("{value:.7}");
    text.trim_end_matches('0').trim_end_matches('.').to_owned()
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

        let location = match (self.glat, self.glong) {
            (None, None) => None,
            (Some(lat), Some(long)) => {
                if lat == 0.0 && long == 0.0 {
                    return Err(bad_request("Koordinat lokasi tidak valid."));
                }
                Some((coordinate_text(lat), coordinate_text(long)))
            }
            _ => return Err(bad_request("Koordinat lokasi harus diisi berpasangan.")),
        };
        let radius = match (&location, self.radius) {
            (Some(_), Some(r))
                if r.fract() == 0.0
                    && (f64::from(RADIUS_MIN)..=f64::from(RADIUS_MAX)).contains(&r) =>
            {
                r as i32
            }
            (Some(_), _) => {
                return Err(bad_request("Radius absen harus 10 sampai 1000 meter."));
            }
            (None, None | Some(0.0)) => 0,
            (None, Some(_)) => {
                return Err(bad_request("Radius absen hanya berlaku bila lokasi diisi."));
            }
        };

        Ok(ShiftInput {
            name,
            start,
            end,
            color: self.color.to_uppercase(),
            description: text_or_na(self.description.as_deref()),
            location_name: text_or_na(self.location_name.as_deref()),
            location,
            radius,
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
    pub location_name: Option<String>,
    pub glat: Option<f64>,
    pub glong: Option<f64>,
    pub radius: Option<i32>,
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

/// Legacy coordinates are free text; anything that is not a finite in-range number is "no location".
pub fn parse_coordinate(raw: &str, limit: f64) -> Option<f64> {
    raw.trim()
        .parse::<f64>()
        .ok()
        .filter(|v| v.is_finite() && v.abs() <= limit)
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
    fn minimal_shift_has_no_location() {
        let input = body(json!({})).expect("valid");
        assert_eq!(input.name, "Shift Pagi");
        assert_eq!(input.color, "#3B82F6");
        assert_eq!((input.description.as_str(), input.radius), (NA, 0));
        assert_eq!(input.location, None);
    }

    #[test]
    fn location_rules() {
        let ok = body(json!({"glat": -6.2088, "glong": 106.8456, "radius": 100})).expect("valid");
        assert_eq!(
            ok.location,
            Some(("-6.2088".to_owned(), "106.8456".to_owned()))
        );
        assert_eq!(ok.radius, 100);
        for extra in [
            json!({"glat": -6.2}),
            json!({"glat": 0, "glong": 0, "radius": 100}),
            json!({"glat": 1, "glong": 1}),
            json!({"glat": 1, "glong": 1, "radius": 9}),
            json!({"glat": 1, "glong": 1, "radius": 1001}),
            json!({"glat": 1, "glong": 1, "radius": 50.5}),
            json!({"radius": 100}),
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

    #[test]
    fn coordinate_text_trims_zeros() {
        assert_eq!(coordinate_text(106.0), "106");
        assert_eq!(coordinate_text(-6.20880001), "-6.2088");
        assert_eq!(parse_coordinate("N/A", 90.0), None);
        assert_eq!(parse_coordinate("91", 90.0), None);
    }
}
