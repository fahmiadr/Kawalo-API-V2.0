//! Schedule forecast: a pure port of `generateSchedulePlan` (admin `scheduleGeneratorHelper.js`).
//!
//! Every person gets a similar number of workdays, shift types are varied, one shift per person
//! per day, Malam -> Pagi on consecutive days is avoided and an optional rest rule caps the
//! consecutive workdays. The only randomness is a Mulberry32 shuffle of the roster that fixes the
//! tie-break order, so the same input and seed always give the same plan.
//!
//! Deliberate differences from the JavaScript helper (bug list in
//! `Task/progress/manajemen-jadwal.md` section 3):
//! * shifts are ordered by start hour, then minute, then request order (JS: hour only, DB order);
//! * existing rows take their code from their real shift, also when it was not selected (JS: `S`);
//! * rows from the days before the range (lookback) feed the streak and the Malam -> Pagi rule
//!   without counting as load, so generating week by week keeps the rest rule;
//! * `overwrite` keeps locked rows (past or already checked in) and seeds them like `empty`;
//! * existing rows are seeded by date, shift start, then id (JS: date only, DB order);
//! * a second legacy row of the same person on the same day keeps the streak (JS resets it to 1).

use std::collections::{BTreeMap, BTreeSet};

use chrono::{Days, NaiveDate};
use serde::{Deserialize, Serialize};

/// Inclusive range limit of one forecast (admin rejected `round(diff) + 1 > 92`).
pub const MAX_RANGE_DAYS: i64 = 92;
/// Schedules may not be created further ahead than this.
pub const MAX_DAYS_AHEAD: u64 = 366;
/// Default of the rest rule; the request may pick 1..=31.
pub const DEFAULT_MAX_CONSECUTIVE: u32 = 6;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Deserialize, Serialize)]
#[serde(rename_all = "lowercase")]
pub enum FillMode {
    /// Only fills slots below quota; every existing row stays.
    #[default]
    Empty,
    /// Unlocked rows of the group in the range are deleted and the range is planned again.
    Overwrite,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
pub struct RestDays {
    pub enabled: bool,
    pub max_consecutive: u32,
}

impl Default for RestDays {
    fn default() -> Self {
        Self {
            enabled: true,
            max_consecutive: DEFAULT_MAX_CONSECUTIVE,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
enum Code {
    Pagi,
    Siang,
    Malam,
}

impl Code {
    /// P = 05-11, S = 12-16, M = 17-23 and 00-04, from the start hour only.
    fn from_hour(hour: i32) -> Self {
        match hour {
            5..=11 => Self::Pagi,
            12..=16 => Self::Siang,
            _ => Self::Malam,
        }
    }

    fn index(self) -> usize {
        self as usize
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ShiftSpec {
    pub uid: String,
    pub name: String,
    pub start_hour: i32,
    pub start_min: i32,
    pub quota: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Person {
    pub email: String,
    pub name: String,
    pub avatar_url: Option<String>,
}

/// A schedule row already in the database, in or shortly before the range.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ExistingRow {
    pub id: i64,
    pub date: NaiveDate,
    pub shift_uid: String,
    pub email: String,
    /// Start of the row's own shift; `None` when that shift no longer exists.
    pub shift_start: Option<(i32, i32)>,
    /// Past, or already checked in or out: never deleted or replaced.
    pub locked: bool,
}

#[derive(Debug, Clone)]
pub struct Input<'a> {
    pub start: NaiveDate,
    pub end: NaiveDate,
    /// Request order; unique uids.
    pub shifts: &'a [ShiftSpec],
    /// Request order after de-duplication.
    pub personnel: &'a [Person],
    /// Rows of the whole group dated before `start` (lookback) or inside the range.
    pub existing: &'a [ExistingRow],
    pub fill_mode: FillMode,
    pub rest: RestDays,
    pub seed: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PlanItem {
    pub date: String,
    pub shift_uid: String,
    pub email: String,
    pub name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct ShiftStat {
    pub shift_uid: String,
    pub shift_name: String,
    pub created: u32,
    pub capacity: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct PersonStat {
    pub email: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct UnderStaffed {
    pub date: String,
    pub shift_uid: String,
    pub shift_name: String,
    pub short: u32,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Stats {
    pub total_created: u32,
    pub total_days: u32,
    pub total_personnel: u32,
    pub total_shifts: u32,
    pub total_capacity: u32,
    pub per_shift: Vec<ShiftStat>,
    pub per_personnel: Vec<PersonStat>,
    pub load_min: u32,
    pub load_max: u32,
    pub under_staffed: Vec<UnderStaffed>,
    pub under_staffed_total: u32,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Plan {
    pub items: Vec<PlanItem>,
    /// Rows `overwrite` deletes (unlocked, in range), ascending.
    pub delete_ids: Vec<i64>,
    pub existing_count: u32,
    pub locked_count: u32,
    pub stats: Stats,
}

/// Business checks of the requested range; the message goes to the user as a 400.
///
/// # Errors
/// The Indonesian reason the range is refused.
pub fn check_range(start: NaiveDate, end: NaiveDate, today: NaiveDate) -> Result<(), String> {
    if end < start {
        return Err("Tanggal akhir tidak boleh sebelum tanggal mulai.".to_owned());
    }
    if (end - start).num_days() + 1 > MAX_RANGE_DAYS {
        return Err(format!("Rentang tanggal maksimal {MAX_RANGE_DAYS} hari."));
    }
    if start < today {
        return Err("Tanggal mulai tidak boleh sebelum hari ini.".to_owned());
    }
    if today
        .checked_add_days(Days::new(MAX_DAYS_AHEAD))
        .is_none_or(|limit| end > limit)
    {
        return Err(format!(
            "Jadwal hanya bisa dibuat sampai {MAX_DAYS_AHEAD} hari ke depan."
        ));
    }
    Ok(())
}

/// Members ticked by default in the forecast form (user decision 3): GUARD only.
pub fn default_selection<T>(members: &[T], role: impl Fn(&T) -> &str) -> Vec<&T> {
    members
        .iter()
        .filter(|m| role(m).trim().eq_ignore_ascii_case("GUARD"))
        .collect()
}

/// Mulberry32, bit-for-bit the JavaScript `makeRng` (u32 wrapping = `| 0` / `Math.imul`).
struct Mulberry32(u32);

impl Mulberry32 {
    fn new(seed: u32) -> Self {
        Self(if seed == 0 { 1 } else { seed })
    }

    fn next_f64(&mut self) -> f64 {
        self.0 = self.0.wrapping_add(0x6D2B_79F5);
        let s = self.0;
        let mut t = (s ^ (s >> 15)).wrapping_mul(1 | s);
        t = t.wrapping_add((t ^ (t >> 7)).wrapping_mul(61 | t)) ^ t;
        f64::from(t ^ (t >> 14)) / 4_294_967_296.0
    }
}

/// Fisher-Yates exactly like the JavaScript `shuffle`.
fn shuffle<T>(items: &mut [T], rng: &mut Mulberry32) {
    for i in (1..items.len()).rev() {
        // `rng < 1`, so the product is below `i + 1`; truncation is `Math.floor` for positives.
        #[allow(clippy::cast_possible_truncation, clippy::cast_sign_loss)]
        let j = (rng.next_f64() * (i + 1) as f64) as usize;
        items.swap(i, j);
    }
}

#[derive(Debug, Clone)]
struct PersonState {
    assigned: u32,
    code_counts: [u32; 3],
    last_day: i64,
    last_code: Option<Code>,
    streak: u32,
    busy: BTreeSet<i64>,
}

impl PersonState {
    fn new() -> Self {
        Self {
            assigned: 0,
            code_counts: [0; 3],
            last_day: -10_000,
            last_code: None,
            streak: 0,
            busy: BTreeSet::new(),
        }
    }

    fn eligible(&self, day: i64, code: Code, rest: RestDays) -> bool {
        let worked_yesterday = self.last_day == day - 1;
        !self.busy.contains(&day)
            && !(code == Code::Pagi && worked_yesterday && self.last_code == Some(Code::Malam))
            && !(rest.enabled && worked_yesterday && self.streak >= rest.max_consecutive)
    }

    /// `applyAssignment`; `counts` is false for lookback rows, which shape the streak only.
    fn apply(&mut self, day: i64, code: Code, counts: bool) {
        if counts {
            self.assigned += 1;
            self.code_counts[code.index()] += 1;
            self.busy.insert(day);
        }
        if self.last_day == day - 1 {
            self.streak += 1;
        } else if self.last_day != day {
            self.streak = 1;
        }
        self.last_day = day;
        self.last_code = Some(code);
    }
}

fn fmt_date(date: NaiveDate) -> String {
    date.format("%Y-%m-%d").to_string()
}

fn count(n: usize) -> u32 {
    u32::try_from(n).unwrap_or(u32::MAX)
}

/// Builds the plan. The caller validates the range ([`check_range`]), the shifts and the members.
pub fn generate(input: &Input<'_>) -> Plan {
    let total_days = (input.end - input.start).num_days() + 1;
    let day_of = |date: NaiveDate| (date - input.start).num_days();

    let mut states: Vec<PersonState> = vec![PersonState::new(); input.personnel.len()];
    let person_of: BTreeMap<String, usize> = input
        .personnel
        .iter()
        .enumerate()
        .map(|(i, p)| (p.email.to_lowercase(), i))
        .collect();
    let shift_of: BTreeMap<&str, usize> = input
        .shifts
        .iter()
        .enumerate()
        .map(|(i, s)| (s.uid.as_str(), i))
        .collect();
    let len_days = usize::try_from(total_days.max(0)).unwrap_or(0);
    let mut occupancy = vec![vec![0_u32; input.shifts.len()]; len_days];

    let mut rows: Vec<&ExistingRow> = input
        .existing
        .iter()
        .filter(|r| r.date <= input.end)
        .collect();
    rows.sort_by_key(|r| (r.date, r.shift_start.unwrap_or((99, 99)), r.id));

    let (mut existing_count, mut locked_count, mut delete_ids) = (0, 0, Vec::new());
    for row in rows {
        let day = day_of(row.date);
        let in_range = day >= 0;
        if in_range {
            existing_count += 1;
            if row.locked {
                locked_count += 1;
            } else if input.fill_mode == FillMode::Overwrite {
                delete_ids.push(row.id);
                continue;
            }
            if let (Some(&s), Ok(d)) = (shift_of.get(row.shift_uid.as_str()), usize::try_from(day))
            {
                occupancy[d][s] += 1;
            }
        }
        if let Some(&p) = person_of.get(&row.email.to_lowercase()) {
            let code = row
                .shift_start
                .map_or(Code::Siang, |(h, _)| Code::from_hour(h));
            states[p].apply(day, code, in_range);
        }
    }
    delete_ids.sort_unstable();

    let mut rng = Mulberry32::new(input.seed);
    let mut roster: Vec<usize> = (0..input.personnel.len()).collect();
    shuffle(&mut roster, &mut rng);

    let mut order: Vec<usize> = (0..input.shifts.len()).collect();
    order.sort_by_key(|&i| (input.shifts[i].start_hour, input.shifts[i].start_min));

    let mut items = Vec::new();
    let mut created_per_shift = vec![0_u32; input.shifts.len()];
    let mut under_staffed = Vec::new();
    for (d, date) in input.start.iter_days().take(len_days).enumerate() {
        let day = i64::try_from(d).unwrap_or(i64::MAX);
        for &s in &order {
            let shift = &input.shifts[s];
            let need = shift.quota.saturating_sub(occupancy[d][s]);
            if need == 0 {
                continue;
            }
            let code = Code::from_hour(shift.start_hour);
            let mut candidates: Vec<usize> = roster
                .iter()
                .copied()
                .filter(|&p| states[p].eligible(day, code, input.rest))
                .collect();
            // Stable, like `Array.prototype.sort`: ties keep the shuffled roster order.
            candidates.sort_by_key(|&p| (states[p].assigned, states[p].code_counts[code.index()]));
            candidates.truncate(need as usize);
            for &p in &candidates {
                states[p].apply(day, code, true);
                items.push(PlanItem {
                    date: fmt_date(date),
                    shift_uid: shift.uid.clone(),
                    email: input.personnel[p].email.clone(),
                    name: input.personnel[p].name.clone(),
                });
            }
            created_per_shift[s] += count(candidates.len());
            let short = need - count(candidates.len());
            if short > 0 {
                under_staffed.push(UnderStaffed {
                    date: fmt_date(date),
                    shift_uid: shift.uid.clone(),
                    shift_name: shift.name.clone(),
                    short,
                });
            }
        }
    }

    let days = count(len_days);
    let per_shift: Vec<ShiftStat> = input
        .shifts
        .iter()
        .zip(&created_per_shift)
        .map(|(s, &created)| ShiftStat {
            shift_uid: s.uid.clone(),
            shift_name: s.name.clone(),
            created,
            capacity: s.quota * days,
        })
        .collect();
    let mut per_personnel: Vec<PersonStat> = input
        .personnel
        .iter()
        .zip(&states)
        .map(|(p, st)| PersonStat {
            email: p.email.clone(),
            name: p.name.clone(),
            avatar_url: p.avatar_url.clone(),
            total: st.assigned,
        })
        .collect();
    per_personnel.sort_by_key(|p| std::cmp::Reverse(p.total));
    let stats = Stats {
        total_created: count(items.len()),
        total_days: days,
        total_personnel: count(input.personnel.len()),
        total_shifts: count(input.shifts.len()),
        total_capacity: per_shift.iter().map(|s| s.capacity).sum(),
        load_min: per_personnel.iter().map(|p| p.total).min().unwrap_or(0),
        load_max: per_personnel.iter().map(|p| p.total).max().unwrap_or(0),
        under_staffed_total: under_staffed.iter().map(|u| u.short).sum(),
        per_shift,
        per_personnel,
        under_staffed,
    };
    Plan {
        items,
        delete_ids,
        existing_count,
        locked_count,
        stats,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn day(s: &str) -> NaiveDate {
        NaiveDate::parse_from_str(s, "%Y-%m-%d").expect("date")
    }

    fn shift(uid: &str, hour: i32, quota: u32) -> ShiftSpec {
        ShiftSpec {
            uid: uid.to_owned(),
            name: format!("N{uid}"),
            start_hour: hour,
            start_min: 0,
            quota,
        }
    }

    fn people(names: &[&str]) -> Vec<Person> {
        names
            .iter()
            .map(|n| Person {
                email: format!("{n}@t.id"),
                name: n.to_uppercase(),
                avatar_url: None,
            })
            .collect()
    }

    fn row(id: i64, date: &str, uid: &str, who: &str, hour: i32, locked: bool) -> ExistingRow {
        ExistingRow {
            id,
            date: day(date),
            shift_uid: uid.to_owned(),
            email: format!("{who}@t.id"),
            shift_start: Some((hour, 0)),
            locked,
        }
    }

    struct Case {
        start: &'static str,
        end: &'static str,
        shifts: Vec<ShiftSpec>,
        personnel: Vec<Person>,
        existing: Vec<ExistingRow>,
        fill_mode: FillMode,
        rest: RestDays,
        seed: u32,
    }

    impl Case {
        fn new(
            start: &'static str,
            end: &'static str,
            shifts: Vec<ShiftSpec>,
            who: &[&str],
        ) -> Self {
            Self {
                start,
                end,
                shifts,
                personnel: people(who),
                existing: Vec::new(),
                fill_mode: FillMode::Empty,
                rest: RestDays::default(),
                seed: 1,
            }
        }

        fn run(&self) -> Plan {
            generate(&Input {
                start: day(self.start),
                end: day(self.end),
                shifts: &self.shifts,
                personnel: &self.personnel,
                existing: &self.existing,
                fill_mode: self.fill_mode,
                rest: self.rest,
                seed: self.seed,
            })
        }
    }

    /// `date|shift|first letter of the email`, the format of the Node golden output.
    fn compact(plan: &Plan) -> Vec<String> {
        plan.items
            .iter()
            .map(|i| format!("{}|{}|{}", i.date, i.shift_uid, &i.email[..1]))
            .collect()
    }

    fn loads(plan: &Plan) -> Vec<String> {
        plan.stats
            .per_personnel
            .iter()
            .map(|p| format!("{}:{}", &p.email[..1], p.total))
            .collect()
    }

    fn short(plan: &Plan) -> Vec<String> {
        plan.stats
            .under_staffed
            .iter()
            .map(|u| format!("{}|{}|{}", u.date, u.shift_uid, u.short))
            .collect()
    }

    #[test]
    fn mulberry32_matches_javascript() {
        // Printed by the JS `makeRng` in Node.
        let expected: [(u32, [f64; 5]); 2] = [
            (
                1,
                [
                    0.627_073_940_588_161_3,
                    0.002_735_721_180_215_478,
                    0.527_447_039_959_952_2,
                    0.981_050_967_471_674_1,
                    0.968_377_898_214_384_9,
                ],
            ),
            (
                12345,
                [
                    0.979_728_267_760_947_3,
                    0.306_752_264_499_664_3,
                    0.484_205_421_525_985,
                    0.817_934_412_509_203,
                    0.509_428_369_347_006_1,
                ],
            ),
        ];
        for (seed, values) in expected {
            let mut rng = Mulberry32::new(seed);
            for v in values {
                assert_eq!(rng.next_f64().to_bits(), v.to_bits(), "seed {seed}");
            }
        }
        // Seed 0 behaves like seed 1 (`(seed >>> 0) || 1`).
        assert_eq!(
            Mulberry32::new(0).next_f64().to_bits(),
            Mulberry32::new(1).next_f64().to_bits()
        );
    }

    // Golden plans: the same configurations through `generateSchedulePlan` in Node.
    #[test]
    fn golden_single_shift() {
        let mut c = Case::new(
            "2026-11-12",
            "2026-11-18",
            vec![shift("p", 7, 2)],
            &["a", "b", "c", "d"],
        );
        c.seed = 12345;
        let plan = c.run();
        assert_eq!(
            compact(&plan),
            [
                "2026-11-12|p|b",
                "2026-11-12|p|c",
                "2026-11-13|p|a",
                "2026-11-13|p|d",
                "2026-11-14|p|b",
                "2026-11-14|p|c",
                "2026-11-15|p|a",
                "2026-11-15|p|d",
                "2026-11-16|p|b",
                "2026-11-16|p|c",
                "2026-11-17|p|a",
                "2026-11-17|p|d",
                "2026-11-18|p|b",
                "2026-11-18|p|c",
            ]
        );
        assert_eq!(loads(&plan), ["b:4", "c:4", "a:3", "d:3"]);
        assert_eq!((plan.stats.load_min, plan.stats.load_max), (3, 4));
        assert!(plan.stats.under_staffed.is_empty());
        assert_eq!(plan.stats.total_capacity, 14);
    }

    #[test]
    fn golden_three_shifts_with_rest_rule() {
        let mut c = Case::new(
            "2026-11-20",
            "2026-12-03",
            vec![shift("m", 22, 2), shift("p", 7, 2), shift("s", 14, 1)],
            &["a", "b", "c", "d", "e", "f"],
        );
        c.rest.max_consecutive = 3;
        let plan = c.run();
        assert_eq!(
            compact(&plan),
            [
                "2026-11-20|p|e",
                "2026-11-20|p|b",
                "2026-11-20|s|f",
                "2026-11-20|m|c",
                "2026-11-20|m|a",
                "2026-11-21|p|d",
                "2026-11-21|p|f",
                "2026-11-21|s|e",
                "2026-11-21|m|b",
                "2026-11-21|m|c",
                "2026-11-22|p|a",
                "2026-11-22|p|d",
                "2026-11-22|s|b",
                "2026-11-22|m|e",
                "2026-11-22|m|f",
                "2026-11-23|p|c",
                "2026-11-23|p|a",
                "2026-11-23|s|d",
                "2026-11-24|p|e",
                "2026-11-24|p|b",
                "2026-11-24|s|c",
                "2026-11-24|m|f",
                "2026-11-24|m|a",
                "2026-11-25|p|d",
                "2026-11-25|p|c",
                "2026-11-25|s|e",
                "2026-11-25|m|b",
                "2026-11-25|m|f",
                "2026-11-26|p|a",
                "2026-11-26|p|d",
                "2026-11-26|s|b",
                "2026-11-26|m|e",
                "2026-11-26|m|f",
                "2026-11-27|p|c",
                "2026-11-27|p|a",
                "2026-11-27|s|d",
                "2026-11-28|p|f",
                "2026-11-28|p|e",
                "2026-11-28|s|a",
                "2026-11-28|m|b",
                "2026-11-28|m|c",
                "2026-11-29|p|d",
                "2026-11-29|p|f",
                "2026-11-29|s|c",
                "2026-11-29|m|e",
                "2026-11-29|m|b",
                "2026-11-30|p|a",
                "2026-11-30|p|d",
                "2026-11-30|s|f",
                "2026-11-30|m|e",
                "2026-11-30|m|b",
                "2026-12-01|p|c",
                "2026-12-01|p|a",
                "2026-12-01|s|d",
                "2026-12-02|p|b",
                "2026-12-02|p|e",
                "2026-12-02|s|a",
                "2026-12-02|m|c",
                "2026-12-02|m|f",
                "2026-12-03|p|d",
                "2026-12-03|p|b",
                "2026-12-03|s|e",
                "2026-12-03|m|c",
                "2026-12-03|m|f",
            ]
        );
        assert_eq!(
            loads(&plan),
            ["b:11", "c:11", "e:11", "f:11", "a:10", "d:10"]
        );
        assert_eq!(
            short(&plan),
            ["2026-11-23|m|2", "2026-11-27|m|2", "2026-12-01|m|2"]
        );
        assert_eq!(plan.stats.under_staffed_total, 6);
    }

    #[test]
    fn golden_fewer_people_than_quota() {
        let mut c = Case::new(
            "2026-12-30",
            "2027-01-01",
            vec![shift("p", 7, 3), shift("m", 23, 2)],
            &["a", "b", "c"],
        );
        c.rest.enabled = false;
        c.seed = 99;
        let plan = c.run();
        assert_eq!(
            compact(&plan),
            [
                "2026-12-30|p|c",
                "2026-12-30|p|b",
                "2026-12-30|p|a",
                "2026-12-31|p|c",
                "2026-12-31|p|b",
                "2026-12-31|p|a",
                "2027-01-01|p|c",
                "2027-01-01|p|b",
                "2027-01-01|p|a",
            ]
        );
        assert_eq!(
            short(&plan),
            ["2026-12-30|m|2", "2026-12-31|m|2", "2027-01-01|m|2"]
        );
        assert_eq!(plan.stats.per_shift[1].capacity, 6);
    }

    #[test]
    fn golden_empty_mode_with_existing_rows() {
        let mut c = Case::new(
            "2026-11-12",
            "2026-11-16",
            vec![shift("p", 6, 2), shift("m", 19, 1)],
            &["a", "b", "c", "d"],
        );
        c.existing = vec![
            row(1, "2026-11-12", "p", "a", 6, false),
            row(2, "2026-11-13", "m", "b", 19, false),
            row(3, "2026-11-14", "p", "x", 6, false),
        ];
        c.seed = 7;
        let plan = c.run();
        assert_eq!(
            compact(&plan),
            [
                "2026-11-12|p|c",
                "2026-11-12|m|d",
                "2026-11-13|p|c",
                "2026-11-13|p|a",
                "2026-11-14|p|d",
                "2026-11-14|m|b",
                "2026-11-15|p|d",
                "2026-11-15|p|c",
                "2026-11-15|m|a",
                "2026-11-16|p|b",
                "2026-11-16|p|d",
                "2026-11-16|m|c",
            ]
        );
        assert_eq!(loads(&plan), ["c:4", "d:4", "a:3", "b:3"]);
        assert_eq!((plan.existing_count, plan.locked_count), (3, 0));
        assert!(plan.delete_ids.is_empty());
    }

    fn busy_case() -> Case {
        let mut c = Case::new(
            "2026-11-01",
            "2026-11-30",
            vec![shift("p", 7, 2), shift("s", 13, 2), shift("m", 21, 2)],
            &["a", "b", "c", "d", "e", "f", "g", "h"],
        );
        c.seed = 4242;
        c
    }

    #[test]
    fn same_seed_same_plan_and_other_seed_differs() {
        let c = busy_case();
        assert_eq!(c.run(), c.run());
        let mut other = busy_case();
        other.seed = 4243;
        assert_ne!(compact(&c.run()), compact(&other.run()));
    }

    #[test]
    fn one_shift_per_person_per_day_and_no_night_then_morning() {
        let plan = busy_case().run();
        let mut seen = BTreeSet::new();
        let mut by_person: BTreeMap<&str, Vec<(&str, &str)>> = BTreeMap::new();
        for i in &plan.items {
            assert!(seen.insert((i.date.clone(), i.email.clone())), "{i:?}");
            by_person
                .entry(&i.email)
                .or_default()
                .push((&i.date, &i.shift_uid));
        }
        for days in by_person.values() {
            for w in days.windows(2) {
                let (d0, d1) = (day(w[0].0), day(w[1].0));
                if (d1 - d0).num_days() == 1 && w[0].1 == "m" {
                    assert_ne!(w[1].1, "p", "{days:?}");
                }
            }
        }
    }

    fn max_streak(plan: &Plan, email: &str) -> i64 {
        let days: Vec<NaiveDate> = plan
            .items
            .iter()
            .filter(|i| i.email == email)
            .map(|i| day(&i.date))
            .collect();
        let (mut best, mut run) = (0, 0);
        for (i, d) in days.iter().enumerate() {
            run = if i > 0 && (*d - days[i - 1]).num_days() == 1 {
                run + 1
            } else {
                1
            };
            best = best.max(run);
        }
        best
    }

    #[test]
    fn rest_rule_caps_consecutive_days_at_six() {
        // Two people, one daily slot each: without the rule both would work every day.
        let mut c = Case::new(
            "2026-11-01",
            "2026-11-30",
            vec![shift("p", 7, 2)],
            &["a", "b", "c"],
        );
        let plan = c.run();
        for p in &c.personnel {
            assert!(max_streak(&plan, &p.email) <= 6, "{}", p.email);
        }
        c.personnel.truncate(2);
        let plan = c.run();
        assert_eq!(max_streak(&plan, "a@t.id"), 6);
        assert!(!plan.stats.under_staffed.is_empty());
        c.rest.enabled = false;
        assert_eq!(max_streak(&c.run(), "a@t.id"), 30);
    }

    #[test]
    fn lookback_rows_continue_the_streak_without_counting_as_load() {
        let mut c = Case::new("2026-11-10", "2026-11-12", vec![shift("p", 7, 1)], &["a"]);
        // Five days in a row right before the range: one more day, then a day off.
        c.existing = (5..=9)
            .map(|d| row(i64::from(d), &format!("2026-11-{d:02}"), "p", "a", 7, true))
            .collect();
        let plan = c.run();
        assert_eq!(compact(&plan), ["2026-11-10|p|a", "2026-11-12|p|a"]);
        assert_eq!(plan.stats.per_personnel[0].total, 2);
        assert_eq!(plan.existing_count, 0);

        // A night shift the evening before blocks the first morning.
        c.existing = vec![row(1, "2026-11-09", "m", "a", 22, true)];
        assert_eq!(compact(&c.run()), ["2026-11-11|p|a", "2026-11-12|p|a"]);
    }

    #[test]
    fn quota_is_respected_including_existing_occupancy() {
        let mut c = busy_case();
        c.existing = vec![row(9, "2026-11-03", "s", "zz", 13, false)];
        let plan = c.run();
        let mut slots: BTreeMap<(String, String), u32> = BTreeMap::new();
        for i in &plan.items {
            *slots
                .entry((i.date.clone(), i.shift_uid.clone()))
                .or_default() += 1;
        }
        assert!(slots.values().all(|&n| n <= 2));
        assert_eq!(slots[&("2026-11-03".to_owned(), "s".to_owned())], 1);
        assert_eq!(plan.stats.total_created, 30 * 6 - 1);
        assert!(plan.stats.per_shift.iter().all(|s| s.created <= s.capacity));
    }

    #[test]
    fn empty_mode_leaves_filled_cells_alone() {
        let mut c = Case::new(
            "2026-11-12",
            "2026-11-13",
            vec![shift("p", 7, 1)],
            &["a", "b"],
        );
        c.existing = vec![row(5, "2026-11-12", "p", "b", 7, false)];
        let plan = c.run();
        assert!(
            plan.items.iter().all(|i| i.date != "2026-11-12"),
            "{plan:?}"
        );
        assert!(plan.delete_ids.is_empty());
        // `b` already works the 12th; the load balances to `a` the next day.
        assert_eq!(compact(&plan), ["2026-11-13|p|a"]);
    }

    #[test]
    fn overwrite_replaces_only_unlocked_rows() {
        let mut c = Case::new(
            "2026-11-12",
            "2026-11-13",
            vec![shift("p", 7, 1)],
            &["a", "b"],
        );
        c.fill_mode = FillMode::Overwrite;
        c.existing = vec![
            row(7, "2026-11-12", "p", "b", 7, false),
            row(3, "2026-11-13", "p", "a", 7, true),
            row(4, "2026-11-13", "x", "b", 13, false),
        ];
        let plan = c.run();
        assert_eq!(plan.delete_ids, [4, 7]);
        assert_eq!((plan.existing_count, plan.locked_count), (3, 1));
        // The locked row keeps the 13th's slot; the 12th is planned again.
        assert_eq!(plan.items.len(), 1);
        assert_eq!(plan.items[0].date, "2026-11-12");
    }

    #[test]
    fn range_checks() {
        let today = day("2026-11-12");
        assert!(check_range(today, day("2027-02-11"), today).is_ok()); // 92 days inclusive
        assert_eq!(
            check_range(today, day("2027-02-12"), today),
            Err("Rentang tanggal maksimal 92 hari.".to_owned())
        );
        assert!(check_range(day("2026-11-13"), today, today).is_err());
        assert!(check_range(day("2026-11-11"), today, today).is_err());
        assert!(check_range(day("2027-11-13"), day("2027-11-13"), today).is_ok());
        assert!(check_range(day("2027-11-14"), day("2027-11-14"), today).is_err());
    }

    #[test]
    fn guards_are_ticked_by_default() {
        let members = [
            ("a@t.id", "GUARD"),
            ("b@t.id", "ADMIN"),
            ("c@t.id", "guard"),
            ("d@t.id", "USER"),
        ];
        let picked: Vec<&str> = default_selection(&members, |m| m.1)
            .into_iter()
            .map(|m| m.0)
            .collect();
        assert_eq!(picked, ["a@t.id", "c@t.id"]);
    }
}
