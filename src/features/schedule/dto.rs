use chrono::{Days, NaiveDate};
use serde::Deserialize;

use crate::core::extract::{Validate, ValidationErrors};

const DEFAULT_SPAN_DAYS: u64 = 30;
const MAX_SPAN_DAYS: i64 = 62;

#[derive(Debug, Deserialize)]
pub struct ScheduleQuery {
    from: Option<String>,
    to: Option<String>,
}

/// Strict `YYYY-MM-DD`; chrono alone would also accept unpadded fields such as `2026-6-3`.
pub(super) fn parse_date(raw: &str) -> Option<NaiveDate> {
    if raw.len() != 10 {
        return None;
    }
    NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok()
}

/// Inclusive date range, with `from` defaulting to `today` and `to` to `from` + 30 days.
pub(super) fn resolve_range(
    from: Option<&str>,
    to: Option<&str>,
    today: NaiveDate,
) -> Result<(NaiveDate, NaiveDate), ValidationErrors> {
    let mut errors = ValidationErrors::new();
    let mut date = |path: &str, raw: Option<&str>| {
        let parsed = raw.and_then(parse_date);
        if raw.is_some() && parsed.is_none() {
            errors.push(path, format!("'{path}' harus berformat YYYY-MM-DD"));
        }
        parsed
    };
    let from = date("from", from);
    let to = date("to", to);
    let from = from.unwrap_or(today);
    let to = to.or_else(|| from.checked_add_days(Days::new(DEFAULT_SPAN_DAYS)));
    let Some(to) = to else {
        errors.push("to", "Rentang tanggal tidak valid");
        return Err(errors);
    };
    if errors.is_empty() {
        if to < from {
            errors.push("to", "'to' tidak boleh sebelum 'from'");
        } else if (to - from).num_days() > MAX_SPAN_DAYS {
            errors.push(
                "to",
                format!("Rentang tanggal maksimal {MAX_SPAN_DAYS} hari"),
            );
        }
    }
    errors.into_result().map(|()| (from, to))
}

impl ScheduleQuery {
    pub fn resolve(&self, today: NaiveDate) -> Result<(NaiveDate, NaiveDate), ValidationErrors> {
        resolve_range(self.from.as_deref(), self.to.as_deref(), today)
    }
}

impl Validate for ScheduleQuery {
    fn validate(&self) -> Result<(), ValidationErrors> {
        self.resolve(chrono::Local::now().date_naive()).map(drop)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(qs: &str) -> ScheduleQuery {
        serde_urlencoded::from_str(qs).expect("query")
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("date")
    }

    #[test]
    fn defaults_and_bounds() {
        let today = day(2026, 11, 10);
        assert_eq!(query("").resolve(today), Ok((today, day(2026, 12, 10))));
        assert_eq!(
            query("from=2026-01-31").resolve(today),
            Ok((day(2026, 1, 31), day(2026, 3, 2)))
        );
        assert!(
            query("from=2026-01-01&to=2026-03-04")
                .resolve(today)
                .is_ok()
        );
        for bad in [
            "from=2026-01-01&to=2026-03-05",
            "from=2026-02-02&to=2026-02-01",
            "from=2026-6-3",
            "from=2026-02-30",
            "to=nope",
            "to=2026-11-09",
        ] {
            assert!(query(bad).resolve(today).is_err(), "{bad}");
        }
    }
}
