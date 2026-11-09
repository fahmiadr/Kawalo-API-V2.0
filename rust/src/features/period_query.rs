//! Query string shared by the read-only list endpoints (`/attendance`, `/operational`).

use std::collections::HashMap;

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;

use crate::core::clean::{clean_date, clean_naive_datetime, clean_string};
use crate::core::constants::MAX_PAGE_SIZE;
use crate::core::extract::{Validate, ValidationErrors};
use crate::core::pagination::{Pagination, PaginationParams, resolve_pagination};

/// `limit`/`offset` as Elysia's `t.Numeric` reads them.
#[derive(Debug, Clone, Copy, PartialEq)]
enum Numeric {
    Absent,
    Value(f64),
    Invalid,
}

impl Numeric {
    fn parse(raw: Option<&String>) -> Self {
        match raw {
            None => Self::Absent,
            Some(text) => js_numeric(text).map_or(Self::Invalid, Self::Value),
        }
    }

    fn value(self) -> Option<f64> {
        match self {
            Self::Value(n) => Some(n),
            Self::Absent | Self::Invalid => None,
        }
    }
}

/// `+text` for a non-empty string, as TypeBox's `numeric` format accepts it: surrounding
/// whitespace, exponents, `0x`/`0o`/`0b` literals and `Infinity`; blank text counts as 0.
fn js_numeric(text: &str) -> Option<f64> {
    if text.is_empty() {
        return None;
    }
    let trimmed = text.trim_matches(|c: char| c.is_whitespace() || c == '\u{feff}');
    match trimmed {
        "" => return Some(0.0),
        "Infinity" | "+Infinity" => return Some(f64::INFINITY),
        "-Infinity" => return Some(f64::NEG_INFINITY),
        _ => {}
    }
    let radix = match trimmed.get(..2) {
        Some("0x" | "0X") => Some(16),
        Some("0o" | "0O") => Some(8),
        Some("0b" | "0B") => Some(2),
        _ => None,
    };
    if let Some(radix) = radix {
        return u128::from_str_radix(&trimmed[2..], radix)
            .ok()
            .map(|n| n as f64);
    }
    // Rust also accepts `inf` and `nan`, which JavaScript reads as NaN.
    if trimmed
        .bytes()
        .any(|b| b.is_ascii_alphabetic() && !matches!(b, b'e' | b'E'))
    {
        return None;
    }
    trimmed.parse::<f64>().ok().filter(|n| !n.is_nan())
}

/// Raw query parameters; a repeated parameter keeps its last value, like Elysia.
#[derive(Debug, Deserialize)]
#[serde(from = "HashMap<String, String>")]
pub struct PeriodQuery {
    search: Option<String>,
    limit: Numeric,
    offset: Numeric,
    start_date: Option<String>,
    end_date: Option<String>,
}

impl From<HashMap<String, String>> for PeriodQuery {
    fn from(mut params: HashMap<String, String>) -> Self {
        Self {
            limit: Numeric::parse(params.get("limit")),
            offset: Numeric::parse(params.get("offset")),
            search: params.remove("search"),
            start_date: params.remove("start_date"),
            end_date: params.remove("end_date"),
        }
    }
}

impl Validate for PeriodQuery {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.opt_length("search", self.search.as_deref(), 1..);
        let max_limit = f64::from(MAX_PAGE_SIZE);
        for (path, numeric, max) in [
            ("limit", self.limit, max_limit),
            ("offset", self.offset, f64::MAX),
        ] {
            match numeric {
                Numeric::Invalid => errors.push(
                    path,
                    format!("Property '{path}' should be one of: 'numeric', 'number'"),
                ),
                Numeric::Value(n) => errors.range(path, n, 0.0..=max),
                Numeric::Absent => {}
            }
        }
        errors.into_result()
    }
}

/// A [`PeriodQuery`] after sanitising, scoped to the caller's group.
#[derive(Debug)]
pub struct PeriodFilter<'a> {
    pub group_uid: &'a str,
    pub search: Option<String>,
    pub page: Pagination,
    pub from: Option<DateTime<Utc>>,
    pub to: Option<DateTime<Utc>>,
}

impl PeriodQuery {
    pub fn into_filter(self, group_uid: &str) -> PeriodFilter<'_> {
        let date = |raw: Option<String>| {
            let iso = clean_date(raw.as_deref())?;
            DateTime::parse_from_rfc3339(&iso)
                .ok()
                .map(|d| d.with_timezone(&Utc))
        };
        PeriodFilter {
            group_uid,
            search: clean_string(self.search.as_deref()),
            page: resolve_pagination(PaginationParams {
                limit: self.limit.value(),
                offset: self.offset.value(),
            }),
            from: date(self.start_date),
            to: date(self.end_date),
        }
    }
}

/// ISO 8601 for a timestamp read as text with `CAST(.. AS CHAR)`, because sqlx cannot decode
/// zero dates. Zero dates and the 1970 epoch give `None`.
pub fn clean_timestamp_text(raw: Option<&str>) -> Option<String> {
    let parsed = NaiveDateTime::parse_from_str(raw?, "%Y-%m-%d %H:%M:%S").ok()?;
    clean_naive_datetime(parsed)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn zero_and_epoch_timestamps_are_dropped() {
        assert_eq!(clean_timestamp_text(Some("0000-00-00 00:00:00")), None);
        assert_eq!(clean_timestamp_text(Some("1970-01-01 00:00:00")), None);
        assert_eq!(clean_timestamp_text(None), None);
        assert_eq!(
            clean_timestamp_text(Some("2026-09-18 10:00:00")).as_deref(),
            Some("2026-09-18T10:00:00.000Z")
        );
    }

    #[test]
    fn filter_sanitises_search_dates_and_paging() {
        let query = PeriodQuery::from(HashMap::from([
            ("search".to_owned(), " N/A ".to_owned()),
            ("limit".to_owned(), "0".to_owned()),
            ("start_date".to_owned(), "not a date".to_owned()),
            ("end_date".to_owned(), "2026-09-18T10:00:00Z".to_owned()),
        ]));
        let filter = query.into_filter("g");
        assert_eq!(filter.search, None);
        assert!(filter.page.is_no_limit());
        assert_eq!(filter.from, None);
        assert_eq!(
            filter.to.map(|d| d.to_rfc3339()).as_deref(),
            Some("2026-09-18T10:00:00+00:00")
        );
    }

    #[test]
    fn numeric_parameters_follow_javascript_number_rules() {
        for (text, want) in [
            ("5", Some(5.0)),
            ("2.5", Some(2.5)),
            ("1e1", Some(10.0)),
            ("0x5", Some(5.0)),
            ("0b11", Some(3.0)),
            (" 2 ", Some(2.0)),
            ("   ", Some(0.0)),
            ("-1", Some(-1.0)),
            ("Infinity", Some(f64::INFINITY)),
            ("", None),
            ("abc", None),
            ("inf", None),
            ("nan", None),
            ("0x", None),
            ("-0x5", None),
            ("1_0", None),
        ] {
            assert_eq!(js_numeric(text), want, "{text:?}");
        }
    }

    #[test]
    fn invalid_numbers_fail_validation_and_limits_are_bounded() {
        let query = |limit: &str, offset: &str| {
            PeriodQuery::from(HashMap::from([
                ("limit".to_owned(), limit.to_owned()),
                ("offset".to_owned(), offset.to_owned()),
            ]))
            .validate()
            .is_ok()
        };
        assert!(query("100", "0"));
        assert!(query(" 2 ", "7"));
        assert!(!query("1", "Infinity"));
        assert!(!query("101", "0"));
        assert!(!query("-1", "0"));
        assert!(!query("", "0"));
        assert!(!query("1", "abc"));
        assert!(!query("1", "-1"));
    }
}
