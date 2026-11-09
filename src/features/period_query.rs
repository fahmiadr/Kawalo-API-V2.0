//! Query string shared by the read-only list endpoints (`/attendance`, `/operational`).

use chrono::{DateTime, NaiveDateTime, Utc};
use serde::Deserialize;

use crate::core::clean::{clean_date, clean_naive_datetime, clean_string};
use crate::core::constants::MAX_PAGE_SIZE;
use crate::core::extract::{Numeric, Validate, ValidationErrors};
use crate::core::pagination::{Pagination, PaginationParams, resolve_pagination};

/// Raw query parameters.
#[derive(Debug, Deserialize)]
pub struct PeriodQuery {
    search: Option<String>,
    limit: Option<Numeric>,
    offset: Option<Numeric>,
    start_date: Option<String>,
    end_date: Option<String>,
}

impl Validate for PeriodQuery {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        errors.opt_length("search", self.search.as_deref(), 1..);
        errors.numeric("limit", self.limit, 0.0..=f64::from(MAX_PAGE_SIZE));
        errors.numeric("offset", self.offset, 0.0..);
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
                limit: self.limit.and_then(Numeric::value),
                offset: self.offset.and_then(Numeric::value),
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
        let query: PeriodQuery = serde_urlencoded::from_str(
            "search=%20N%2FA%20&limit=0&start_date=not%20a%20date&end_date=2026-09-18T10%3A00%3A00Z",
        )
        .expect("query");
        let filter = query.into_filter("g");
        assert_eq!(filter.search, None);
        assert!(filter.page.is_no_limit());
        assert_eq!(filter.from, None);
        assert_eq!(
            filter.to.map(|d| d.to_rfc3339()).as_deref(),
            Some("2026-09-18T10:00:00+00:00")
        );
    }
}
