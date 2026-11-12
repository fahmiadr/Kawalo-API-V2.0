//! `GET /attendance/stats`: counts for the report screen, over the caller's active group.

use axum::extract::State;
use chrono::{DateTime, Datelike, Days, Local, NaiveDate, TimeZone, Utc};
use serde::{Deserialize, Serialize};
use sqlx::{MySql, QueryBuilder};

use super::entity::Category;
use crate::core::clean::clean_string;
use crate::core::error::ApiError;
use crate::core::extract::{CurrentUser, Validate, ValidatedQuery, ValidationErrors};
use crate::core::response::{ApiResult, ok};
use crate::core::state::AppState;

const MAX_SPAN_DAYS: i64 = 92;

#[derive(Debug, Deserialize)]
pub struct StatsQuery {
    from: Option<String>,
    to: Option<String>,
    email: Option<String>,
}

/// Strict `YYYY-MM-DD`.
fn parse_date(raw: &str) -> Option<NaiveDate> {
    (raw.len() == 10)
        .then(|| NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok())
        .flatten()
}

impl StatsQuery {
    /// Inclusive local dates; `from` defaults to the first of `today`'s month, `to` to `today`.
    fn resolve(&self, today: NaiveDate) -> Result<(NaiveDate, NaiveDate), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        let mut date = |path: &str, raw: Option<&str>| {
            let parsed = raw.and_then(parse_date);
            if raw.is_some() && parsed.is_none() {
                errors.push(path, format!("'{path}' harus berformat YYYY-MM-DD"));
            }
            parsed
        };
        let from = date("from", self.from.as_deref());
        let to = date("to", self.to.as_deref()).unwrap_or(today);
        let from = from.unwrap_or_else(|| to.with_day(1).unwrap_or(to));
        if let Some(email) = &self.email {
            errors.length("email", email, 3..=128);
        }
        if errors.is_empty() {
            if to < from {
                errors.push("to", "'to' tidak boleh sebelum 'from'");
            } else if (to - from).num_days() >= MAX_SPAN_DAYS {
                errors.push(
                    "to",
                    format!("Rentang tanggal maksimal {MAX_SPAN_DAYS} hari"),
                );
            }
        }
        errors.into_result().map(|()| (from, to))
    }
}

impl Validate for StatsQuery {
    fn validate(&self) -> Result<(), ValidationErrors> {
        self.resolve(Local::now().date_naive()).map(drop)
    }
}

#[derive(Debug, Default, Serialize, PartialEq, Eq)]
pub struct Stats {
    from: String,
    to: String,
    email: Option<String>,
    total: u32,
    check_in: u32,
    check_out: u32,
    other: u32,
    /// Check-ins with late minutes.
    late: u32,
    /// Check-outs before the shift end (beyond the tolerance at the time of check-out).
    early_leave: u32,
}

impl Stats {
    fn add(&mut self, kind: Option<&str>, late_mins: i32, early_mins: i32) {
        self.total += 1;
        match Category::of(kind) {
            Category::Masuk => {
                self.check_in += 1;
                self.late += u32::from(late_mins > 0);
            }
            Category::Pulang => {
                self.check_out += 1;
                self.early_leave += u32::from(early_mins > 0);
            }
            Category::Lainnya => self.other += 1,
        }
    }
}

fn local_midnight_utc(date: NaiveDate) -> Option<DateTime<Utc>> {
    Local
        .from_local_datetime(&date.and_hms_opt(0, 0, 0)?)
        .earliest()
        .map(|d| d.with_timezone(&Utc))
}

/// Rows are classified in Rust with [`Category::of`], the rule `GET /attendance` uses, instead of
/// duplicating it in SQL. ponytail: one pass over at most 92 days of a group; aggregate in SQL if
/// groups grow to thousands of rows per day.
pub async fn stats(
    ValidatedQuery(query): ValidatedQuery<StatsQuery>,
    user: CurrentUser,
    State(state): State<AppState>,
) -> ApiResult<Stats> {
    const MESSAGE: &str = "Ringkasan absensi berhasil diambil";
    let (from, to) = query
        .resolve(Local::now().date_naive())
        .map_err(|errors| ApiError::invalid_input(errors.to_string()))?;
    let email = clean_string(query.email.as_deref());
    let mut stats = Stats {
        from: from.format("%Y-%m-%d").to_string(),
        to: to.format("%Y-%m-%d").to_string(),
        email: email.clone(),
        ..Stats::default()
    };
    let Some(group_uid) = user.group_uid.as_deref().filter(|g| !g.is_empty()) else {
        return ok(stats, MESSAGE);
    };
    let (Some(start), Some(end)) = (
        local_midnight_utc(from),
        to.checked_add_days(Days::new(1))
            .and_then(local_midnight_utc),
    ) else {
        return Err(ApiError::invalid_input("/to: Rentang tanggal tidak valid"));
    };

    let mut sql = QueryBuilder::<MySql>::new(
        "SELECT AbsenType, ADiffMins, AEarlyMins FROM tkabsen WHERE AbsenGroupUID = ",
    );
    sql.push_bind(group_uid)
        .push(" AND AbsenDT >= ")
        .push_bind(start)
        .push(" AND AbsenDT < ")
        .push_bind(end);
    if let Some(email) = &email {
        sql.push(" AND AbsenEmail = ").push_bind(email.as_str());
    }
    let rows: Vec<(Option<String>, i32, i32)> = sql.build_query_as().fetch_all(&state.db).await?;
    for (kind, late, early) in &rows {
        stats.add(kind.as_deref(), *late, *early);
    }
    ok(stats, MESSAGE)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn query(qs: &str) -> StatsQuery {
        serde_urlencoded::from_str(qs).expect("query")
    }

    fn day(y: i32, m: u32, d: u32) -> NaiveDate {
        NaiveDate::from_ymd_opt(y, m, d).expect("date")
    }

    #[test]
    fn range_defaults_to_the_current_month() {
        let today = day(2026, 11, 12);
        assert_eq!(query("").resolve(today), Ok((day(2026, 11, 1), today)));
        assert_eq!(
            query("from=2026-10-01&to=2026-12-31").resolve(today),
            Ok((day(2026, 10, 1), day(2026, 12, 31)))
        );
        for bad in [
            "from=2026-01-01&to=2026-04-03",
            "from=2026-02-02&to=2026-02-01",
            "from=2026-6-3",
            "to=nope",
            "email=a",
        ] {
            assert!(query(bad).resolve(today).is_err(), "{bad}");
        }
    }

    #[test]
    fn rows_are_counted_by_category() {
        let mut stats = Stats::default();
        stats.add(Some("CHECK IN"), 5, 0);
        stats.add(Some("CHECK IN"), 0, 0);
        stats.add(Some("Absen Masuk"), -5, 0);
        stats.add(Some("CHECK OUT"), 0, 12);
        stats.add(Some("CHECK OUT"), 0, 0);
        stats.add(Some("REGISTER"), 0, 0);
        stats.add(None, 3, 3);
        assert_eq!(
            (
                stats.total,
                stats.check_in,
                stats.check_out,
                stats.other,
                stats.late,
                stats.early_leave
            ),
            (7, 3, 2, 2, 1, 1)
        );
    }
}
