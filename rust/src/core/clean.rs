//! Sanitisers for dirty legacy data, ported from `clean-data.util.ts`.
//!
//! TypeScript distinguished `undefined` from `null`; in Rust both are `None`, so the
//! `cleanQuery*` variants collapse into the plain function of the same name:
//!
//! | TypeScript | Rust |
//! |---|---|
//! | `cleanString`, `cleanQueryString` | [`clean_string`] |
//! | `cleanDate`, `cleanQueryDate` | [`clean_date`] (text) or [`clean_datetime`] (timestamp) |
//! | `cleanCoordinate`, `cleanQueryCoordinate` | [`clean_coordinate`] |
//! | `cleanBoolean`, `cleanQueryBoolean` | [`clean_boolean`] |
//! | `cleanNumber`, `cleanInteger`, `cleanQueryInteger` | [`clean_number`], [`clean_integer`] |
//! | `cleanUpdateString/Coordinate/Date` | [`clean_update_string`] and friends on [`Patch`] |
//! | `toLocalDatetime` | [`to_local_datetime`] |

use std::sync::LazyLock;

use chrono::{DateTime, Local, NaiveDate, NaiveDateTime, SecondsFormat, TimeZone, Utc};
use regex::Regex;
use serde::{Deserialize, Deserializer};

use crate::core::extract::numeric::js_number;

/// WHATWG HTML5 email grammar, also used by `cleanString` to lower-case emails.
static EMAIL: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(
        r"^[a-zA-Z0-9.!#$%&'*+/=?^_`{|}~-]+@[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?(?:\.[a-zA-Z0-9](?:[a-zA-Z0-9-]{0,61}[a-zA-Z0-9])?)+$",
    )
    .expect("static regex is valid")
});

/// Leading integer of `raw`, like JavaScript `Number.parseInt(raw, 10)`.
pub(crate) fn parse_int_prefix(raw: &str) -> Option<i64> {
    let s = raw.trim_start();
    let (sign, digits) = match s.as_bytes().first()? {
        b'-' => (-1, &s[1..]),
        b'+' => (1, &s[1..]),
        _ => (1, s),
    };
    let end = digits
        .bytes()
        .position(|b| !b.is_ascii_digit())
        .unwrap_or(digits.len());
    digits[..end].parse::<i64>().ok().map(|n| n * sign)
}

/// Trims, maps empty, `N/A` and `null` to `None`, and lower-cases values that look like emails.
pub fn clean_string(val: Option<&str>) -> Option<String> {
    let trimmed = val?.trim();
    if matches!(trimmed, "" | "N/A" | "null") {
        return None;
    }
    Some(if EMAIL.is_match(trimmed) {
        trimmed.to_lowercase()
    } else {
        trimmed.to_owned()
    })
}

/// `"true"`/`"1"` and `"false"`/`"0"` (case-insensitive); anything else is `None`.
pub fn clean_boolean(val: Option<&str>) -> Option<bool> {
    match val?.trim().to_lowercase().as_str() {
        "true" | "1" => Some(true),
        "false" | "0" => Some(false),
        _ => None,
    }
}

/// Finite number from text; `None` for dirty markers, `NaN` and infinities.
pub fn clean_number(val: Option<&str>) -> Option<f64> {
    js_number(&clean_string(val)?).filter(|n| n.is_finite())
}

/// Like [`clean_number`], truncating the fraction.
pub fn clean_integer(val: Option<&str>) -> Option<i64> {
    clean_number(val).map(|n| n.trunc() as i64)
}

/// Finite number from a number that may be `NaN` or infinite.
pub fn clean_finite(val: Option<f64>) -> Option<f64> {
    val.filter(|n| n.is_finite())
}

/// Integer from a number, truncating the fraction; `None` for `NaN` and infinities.
pub fn clean_integer_f64(val: Option<f64>) -> Option<i64> {
    clean_finite(val).map(|n| n.trunc() as i64)
}

/// `YYYY-MM-DDTHH:mm:ss.sssZ`, the output of `Date.prototype.toISOString`.
pub fn iso_string(date: DateTime<Utc>) -> String {
    date.to_rfc3339_opts(SecondsFormat::Millis, true)
}

fn is_legacy_epoch(iso: &str) -> bool {
    iso.starts_with("1970-01-01")
}

/// ISO 8601 text for a timestamp, `None` for the legacy epoch (1970-01-01).
pub fn clean_datetime(val: DateTime<Utc>) -> Option<String> {
    let iso = iso_string(val);
    (!is_legacy_epoch(&iso)).then_some(iso)
}

/// Like [`clean_datetime`] for a `DATETIME`/`TIMESTAMP` column, which the old Prisma adapter
/// returned interpreted as UTC.
pub fn clean_naive_datetime(val: NaiveDateTime) -> Option<String> {
    clean_datetime(val.and_utc())
}

/// Parses text like `new Date(text)` does for the shapes the app sends, then renders ISO 8601.
/// Zero dates, `N/A`, the 1970 epoch and unparsable text give `None`.
pub fn clean_date(val: Option<&str>) -> Option<String> {
    let trimmed = val?.trim();
    if trimmed.is_empty()
        || trimmed == "N/A"
        || trimmed.starts_with("0000-00-00")
        || trimmed.starts_with("1970-01-01")
    {
        return None;
    }
    clean_datetime(parse_js_date(trimmed)?)
}

/// Date-only text is UTC; date-time text without an offset is local time, as in V8.
fn parse_js_date(text: &str) -> Option<DateTime<Utc>> {
    if let Ok(date) = DateTime::parse_from_rfc3339(text) {
        return Some(date.with_timezone(&Utc));
    }
    let spaced = text.replacen(' ', "T", 1);
    if let Ok(date) = DateTime::parse_from_rfc3339(&spaced) {
        return Some(date.with_timezone(&Utc));
    }
    if let Ok(date) = NaiveDate::parse_from_str(text, "%Y-%m-%d") {
        return Some(date.and_hms_opt(0, 0, 0)?.and_utc());
    }
    for format in ["%Y-%m-%dT%H:%M:%S%.f", "%Y-%m-%dT%H:%M"] {
        if let Ok(naive) = NaiveDateTime::parse_from_str(&spaced, format) {
            return Local
                .from_local_datetime(&naive)
                .earliest()
                .map(|d| d.with_timezone(&Utc));
        }
    }
    None
}

/// Coordinate text, `None` for dirty markers, zero values and non-numeric text.
pub fn clean_coordinate(val: Option<&str>) -> Option<String> {
    let cleaned = clean_string(val)?;
    if matches!(cleaned.as_str(), "0" | "0.0" | "0.000000") {
        return None;
    }
    js_number(&cleaned)?;
    Some(cleaned)
}

/// ISO 8601 text as local `YYYY-MM-DD HH:mm:ss`, the session time zone MySQL compares against.
/// Unparsable text is returned unchanged.
pub fn to_local_datetime(iso: &str) -> String {
    match parse_js_date(iso.trim()) {
        Some(date) => date
            .with_timezone(&Local)
            .format("%Y-%m-%d %H:%M:%S")
            .to_string(),
        None => iso.to_owned(),
    }
}

/// Tri-state field of a PUT/PATCH body: absent (leave unchanged), `null`/dirty (clear), or a value.
///
/// Declare as `#[serde(default)] field: Patch<String>`; a missing key yields [`Patch::Missing`]
/// and a JSON `null` yields [`Patch::Clear`].
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub enum Patch<T> {
    #[default]
    Missing,
    Clear,
    Set(T),
}

impl<'de, T: Deserialize<'de>> Deserialize<'de> for Patch<T> {
    fn deserialize<D: Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        Ok(Option::<T>::deserialize(deserializer)?.map_or(Self::Clear, Self::Set))
    }
}

impl<T> Patch<T> {
    pub fn is_missing(&self) -> bool {
        matches!(self, Self::Missing)
    }

    /// `Some(None)` clears, `Some(Some(v))` sets, `None` leaves unchanged.
    pub fn into_option(self) -> Option<Option<T>> {
        match self {
            Self::Missing => None,
            Self::Clear => Some(None),
            Self::Set(v) => Some(Some(v)),
        }
    }

    fn clean_with(&self, clean: impl FnOnce(&T) -> Option<String>) -> Patch<String> {
        match self {
            Self::Missing => Patch::Missing,
            Self::Clear => Patch::Clear,
            Self::Set(v) => clean(v).map_or(Patch::Clear, Patch::Set),
        }
    }
}

/// [`clean_string`] on a mutation field.
pub fn clean_update_string(val: &Patch<String>) -> Patch<String> {
    val.clean_with(|s| clean_string(Some(s)))
}

/// [`clean_coordinate`] on a mutation field.
pub fn clean_update_coordinate(val: &Patch<String>) -> Patch<String> {
    val.clean_with(|s| clean_coordinate(Some(s)))
}

/// [`clean_date`] on a mutation field.
pub fn clean_update_date(val: &Patch<String>) -> Patch<String> {
    val.clean_with(|s| clean_date(Some(s)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn clean_string_drops_dirty_markers() {
        for dirty in [
            None,
            Some(""),
            Some("   "),
            Some("N/A"),
            Some("  N/A  "),
            Some("null"),
        ] {
            assert_eq!(clean_string(dirty), None, "{dirty:?}");
        }
    }

    #[test]
    fn clean_string_lowercases_only_emails() {
        assert_eq!(
            clean_string(Some("  Satpam1@Kawalo.Id  ")).as_deref(),
            Some("satpam1@kawalo.id")
        );
        assert_eq!(
            clean_string(Some("USER.NAME+TAG@COMPANY.CO.ID")).as_deref(),
            Some("user.name+tag@company.co.id")
        );
        assert_eq!(
            clean_string(Some("Budi Santoso")).as_deref(),
            Some("Budi Santoso")
        );
        assert_eq!(
            clean_string(Some("NOT_AN_EMAIL@")).as_deref(),
            Some("NOT_AN_EMAIL@")
        );
        assert_eq!(
            clean_string(Some("user@localhost")).as_deref(),
            Some("user@localhost")
        );
    }

    #[test]
    fn clean_boolean_accepts_the_documented_spellings() {
        assert_eq!(clean_boolean(Some(" TRUE ")), Some(true));
        assert_eq!(clean_boolean(Some("0")), Some(false));
        assert_eq!(clean_boolean(Some("yes")), None);
        assert_eq!(clean_boolean(None), None);
    }

    #[test]
    fn clean_number_rejects_non_finite_and_rust_only_spellings() {
        assert_eq!(clean_number(Some(" 12.5 ")), Some(12.5));
        assert_eq!(clean_number(Some("1e2")), Some(100.0));
        // `Number()` also reads radix literals.
        assert_eq!(clean_number(Some("0x1A")), Some(26.0));
        for bad in ["abc", "NaN", "inf", "Infinity", "N/A", ""] {
            assert_eq!(clean_number(Some(bad)), None, "{bad}");
        }
        assert_eq!(clean_integer(Some("7.9")), Some(7));
        assert_eq!(clean_integer(Some("-7.9")), Some(-7));
        assert_eq!(clean_integer_f64(Some(f64::NAN)), None);
        assert_eq!(clean_integer_f64(Some(3.99)), Some(3));
    }

    #[test]
    fn clean_date_rejects_legacy_values() {
        for bad in [
            "0000-00-00 00:00:00",
            "1970-01-01 00:00:00",
            "1970-01-01T00:00:00.000Z",
            "N/A",
            "",
            "bukan tanggal",
        ] {
            assert_eq!(clean_date(Some(bad)), None, "{bad}");
        }
        assert_eq!(clean_date(None), None);
    }

    #[test]
    fn clean_date_renders_iso_with_milliseconds() {
        assert_eq!(
            clean_date(Some("2026-09-18T10:00:00.000Z")).as_deref(),
            Some("2026-09-18T10:00:00.000Z")
        );
        assert_eq!(
            clean_date(Some("2026-09-18T10:00:00+07:00")).as_deref(),
            Some("2026-09-18T03:00:00.000Z")
        );
        assert_eq!(
            clean_date(Some("2026-09-18")).as_deref(),
            Some("2026-09-18T00:00:00.000Z")
        );
        assert_eq!(
            clean_date(Some("2026-09-18 10:00:00Z")).as_deref(),
            Some("2026-09-18T10:00:00.000Z")
        );
    }

    #[test]
    fn clean_datetime_skips_the_epoch() {
        assert_eq!(clean_datetime(DateTime::UNIX_EPOCH), None);
        let date = NaiveDate::from_ymd_opt(2026, 9, 18).and_then(|d| d.and_hms_opt(10, 0, 0));
        assert_eq!(
            clean_naive_datetime(date.expect("valid date")).as_deref(),
            Some("2026-09-18T10:00:00.000Z")
        );
    }

    #[test]
    fn clean_coordinate_drops_zero_and_non_numeric() {
        for bad in ["0", "0.0", "0.000000", "N/A", "invalid"] {
            assert_eq!(clean_coordinate(Some(bad)), None, "{bad}");
        }
        assert_eq!(
            clean_coordinate(Some("-6.200000")).as_deref(),
            Some("-6.200000")
        );
        assert_eq!(
            clean_coordinate(Some("106.816666")).as_deref(),
            Some("106.816666")
        );
    }

    #[test]
    fn to_local_datetime_formats_and_passes_garbage_through() {
        assert_eq!(to_local_datetime("not a date"), "not a date");
        let formatted = to_local_datetime("2026-09-18T10:00:00.000Z");
        assert_eq!(formatted.len(), 19, "{formatted}");
        assert_eq!(&formatted[4..5], "-");
    }

    #[test]
    fn patch_distinguishes_missing_null_and_value() {
        #[derive(Deserialize)]
        struct Body {
            #[serde(default)]
            name: Patch<String>,
        }
        let missing: Body = serde_json::from_str("{}").expect("parse");
        let cleared: Body = serde_json::from_str(r#"{"name":null}"#).expect("parse");
        let set: Body = serde_json::from_str(r#"{"name":"  Budi "}"#).expect("parse");
        let blank: Body = serde_json::from_str(r#"{"name":"  "}"#).expect("parse");
        assert_eq!(clean_update_string(&missing.name), Patch::Missing);
        assert_eq!(clean_update_string(&cleared.name), Patch::Clear);
        assert_eq!(
            clean_update_string(&set.name),
            Patch::Set("Budi".to_owned())
        );
        assert_eq!(clean_update_string(&blank.name), Patch::Clear);
        assert_eq!(
            clean_update_coordinate(&Patch::Set("0".into())),
            Patch::Clear
        );
        assert_eq!(set.name.into_option(), Some(Some("  Budi ".to_owned())));
    }
}
