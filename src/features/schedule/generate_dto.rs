//! Bodies of `POST /schedules/generate/preview` and `POST /schedules/generate`.
//!
//! The body has nested objects, which the TypeBox-style [`Schema`] cannot describe, so `parse`
//! is overridden: serde shapes the value, then [`Validate`] checks the limits. Both fail as 422;
//! rules that need the clock or the database (range, shifts, members) are 400 in the handler.

use std::collections::BTreeSet;

use chrono::NaiveDate;
use serde::{Deserialize, Serialize};
use serde_json::Value;

use super::generator::{DEFAULT_MAX_CONSECUTIVE, FillMode, PlanItem, RestDays, Stats};
use crate::core::extract::schema::{Body, Schema};
use crate::core::extract::{Validate, ValidationErrors};

pub const MAX_SHIFTS: usize = 50;
pub const MAX_QUOTA: u32 = 50;
pub const MAX_PERSONNEL: usize = 500;
pub const MAX_CONSECUTIVE_LIMIT: u32 = 31;
pub const MAX_SEED: u32 = i32::MAX as u32;

#[derive(Debug, Clone, Deserialize)]
pub struct ShiftQuota {
    pub shift_uid: String,
    pub quota: u32,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RestDaysBody {
    pub enabled: bool,
    pub max_consecutive: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GenerateBody {
    pub start_date: String,
    pub end_date: String,
    pub shifts: Vec<ShiftQuota>,
    /// Omitted: every JOINED `GUARD` of the group (the form's default selection).
    pub personnel: Option<Vec<String>>,
    #[serde(default)]
    pub fill_mode: FillMode,
    pub rest_days: Option<RestDaysBody>,
    pub seed: Option<u32>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct CommitBody {
    #[serde(flatten)]
    pub params: GenerateBody,
    pub preview_token: String,
    /// Must be `true` for `overwrite`: the client showed how many rows get deleted.
    #[serde(default)]
    pub confirm_overwrite: bool,
}

/// Strict `YYYY-MM-DD`.
pub fn parse_date(raw: &str) -> Option<NaiveDate> {
    (raw.len() == 10)
        .then(|| NaiveDate::parse_from_str(raw, "%Y-%m-%d").ok())
        .flatten()
}

impl GenerateBody {
    pub fn rest(&self) -> RestDays {
        self.rest_days
            .as_ref()
            .map_or_else(RestDays::default, |r| RestDays {
                enabled: r.enabled,
                max_consecutive: r.max_consecutive.unwrap_or(DEFAULT_MAX_CONSECUTIVE),
            })
    }

    /// Validated dates; only call after [`Validate::validate`] passed.
    pub fn dates(&self) -> Option<(NaiveDate, NaiveDate)> {
        Some((parse_date(&self.start_date)?, parse_date(&self.end_date)?))
    }

    /// Trimmed, case-insensitively de-duplicated, first occurrence kept.
    pub fn personnel(&self) -> Option<Vec<String>> {
        let mut seen = BTreeSet::new();
        self.personnel.as_ref().map(|list| {
            list.iter()
                .map(|e| e.trim().to_owned())
                .filter(|e| seen.insert(e.to_lowercase()))
                .collect()
        })
    }
}

impl Validate for GenerateBody {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = ValidationErrors::new();
        for (path, raw) in [
            ("start_date", &self.start_date),
            ("end_date", &self.end_date),
        ] {
            if parse_date(raw).is_none() {
                errors.push(path, format!("'{path}' harus berformat YYYY-MM-DD"));
            }
        }
        if !(1..=MAX_SHIFTS).contains(&self.shifts.len()) {
            errors.push("shifts", format!("Pilih 1 sampai {MAX_SHIFTS} shift"));
        }
        let mut uids = BTreeSet::new();
        for (i, s) in self.shifts.iter().enumerate() {
            if s.shift_uid.trim().is_empty() || !uids.insert(s.shift_uid.trim()) {
                errors.push(
                    &format!("shifts/{i}/shift_uid"),
                    "shift_uid wajib diisi dan tidak boleh ganda",
                );
            }
            if !(1..=MAX_QUOTA).contains(&s.quota) {
                errors.push(
                    &format!("shifts/{i}/quota"),
                    format!("Kuota harus 1 sampai {MAX_QUOTA}"),
                );
            }
        }
        if let Some(list) = &self.personnel {
            if !(1..=MAX_PERSONNEL).contains(&list.len()) {
                errors.push(
                    "personnel",
                    format!("Pilih 1 sampai {MAX_PERSONNEL} personil"),
                );
            }
            if list.iter().any(|e| e.trim().is_empty()) {
                errors.push("personnel", "Email personil tidak boleh kosong");
            }
        }
        if let Some(n) = self.rest_days.as_ref().and_then(|r| r.max_consecutive)
            && !(1..=MAX_CONSECUTIVE_LIMIT).contains(&n)
        {
            errors.push(
                "rest_days/max_consecutive",
                format!("Maksimal hari kerja berturut-turut 1 sampai {MAX_CONSECUTIVE_LIMIT}"),
            );
        }
        if self.seed.is_some_and(|s| s > MAX_SEED) {
            errors.push("seed", format!("seed harus 0 sampai {MAX_SEED}"));
        }
        errors.into_result()
    }
}

impl Validate for CommitBody {
    fn validate(&self) -> Result<(), ValidationErrors> {
        let mut errors = match self.params.validate() {
            Ok(()) => ValidationErrors::new(),
            Err(errors) => errors,
        };
        if self.params.seed.is_none() {
            errors.push("seed", "seed wajib diisi");
        }
        if self.preview_token.trim().is_empty() {
            errors.push("preview_token", "preview_token wajib diisi");
        }
        errors.into_result()
    }
}

/// serde shapes the body, then [`Validate`] checks it; either failure is the 422 detail.
fn parse_validated<T: for<'de> Deserialize<'de> + Validate>(value: Value) -> Result<T, String> {
    let body: T = serde_json::from_value(value).map_err(|e| e.to_string())?;
    body.validate().map_err(|e| e.to_string())?;
    Ok(body)
}

impl Body for GenerateBody {
    const SCHEMA: Schema = Schema::new(&[]);

    fn parse(value: Value) -> Result<Self, String> {
        parse_validated(value)
    }
}

impl Body for CommitBody {
    const SCHEMA: Schema = Schema::new(&[]);

    fn parse(value: Value) -> Result<Self, String> {
        parse_validated(value)
    }
}

#[derive(Debug, Serialize)]
pub struct PreviewResponse {
    pub seed: u32,
    pub fill_mode: FillMode,
    pub preview_token: String,
    pub existing_count: u32,
    pub delete_count: u32,
    pub locked_count: u32,
    pub plan: Vec<PlanItem>,
    pub stats: Stats,
}

#[derive(Debug, Serialize)]
pub struct CommitResponse {
    pub created: u64,
    pub deleted: u64,
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    fn base() -> Value {
        json!({
            "start_date": "2026-11-12",
            "end_date": "2026-11-18",
            "shifts": [{"shift_uid": "s1", "quota": 2}],
        })
    }

    #[test]
    fn defaults_and_dedupe() {
        let mut v = base();
        v["personnel"] = json!([" Budi@x.id ", "budi@x.id", "ani@x.id"]);
        let body = GenerateBody::parse(v).expect("valid");
        assert_eq!(body.fill_mode, FillMode::Empty);
        assert_eq!(body.rest(), RestDays::default());
        assert_eq!(
            body.personnel(),
            Some(vec!["Budi@x.id".to_owned(), "ani@x.id".to_owned()])
        );
    }

    #[test]
    fn limits_are_422_details() {
        for (key, bad) in [
            ("start_date", json!("2026-11-1")),
            ("shifts", json!([])),
            ("shifts", json!([{"shift_uid": "a", "quota": 0}])),
            ("shifts", json!([{"shift_uid": "a", "quota": 51}])),
            (
                "shifts",
                json!([{"shift_uid": "a", "quota": 1}, {"shift_uid": "a", "quota": 1}]),
            ),
            ("personnel", json!([])),
            ("fill_mode", json!("all")),
            ("rest_days", json!({"enabled": true, "max_consecutive": 32})),
            ("seed", json!(2_147_483_648_u64)),
            ("seed", json!(-1)),
        ] {
            let mut v = base();
            v[key] = bad.clone();
            assert!(GenerateBody::parse(v).is_err(), "{key}: {bad}");
        }
        assert!(GenerateBody::parse(Value::Null).is_err());
    }

    #[test]
    fn commit_needs_seed_and_token() {
        let mut v = base();
        assert!(CommitBody::parse(v.clone()).is_err());
        v["seed"] = json!(5);
        v["preview_token"] = json!("ab");
        let body = CommitBody::parse(v).expect("valid");
        assert!(!body.confirm_overwrite);
        assert_eq!(body.params.seed, Some(5));
    }
}
