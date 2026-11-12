//! Pure decision logic for schedule writes: at most one shift per person per day.

use std::collections::HashMap;

use chrono::NaiveDate;

pub struct Candidate {
    /// Lower-cased: the column collation is case-insensitive, so `A@x.id` and `a@x.id` clash.
    pub email_key: String,
    pub shift_uid: String,
    pub date: NaiveDate,
}

pub struct Existing {
    /// `None` for a row planned earlier in the same request.
    pub id: Option<i32>,
    pub shift_uid: String,
    pub locked: bool,
}

pub type Occupancy = HashMap<(String, NaiveDate), Vec<Existing>>;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Skip {
    /// The same shift is already scheduled that day.
    Exists,
    OtherShift,
    /// A row that cannot be replaced because it already has attendance.
    Locked,
}

#[derive(Debug, Default, PartialEq, Eq)]
pub struct Plan {
    pub insert: Vec<usize>,
    pub delete: Vec<i32>,
    pub skipped: Vec<(usize, Skip)>,
}

/// Decides, in input order, which candidates become rows. With `overwrite`, a different shift
/// already on that day is replaced when every row there is unlocked and already stored.
pub fn plan(candidates: &[Candidate], occupancy: &mut Occupancy, overwrite: bool) -> Plan {
    let mut plan = Plan::default();
    for (index, c) in candidates.iter().enumerate() {
        let slot = occupancy.entry((c.email_key.clone(), c.date)).or_default();
        if slot.iter().any(|e| e.shift_uid == c.shift_uid) {
            plan.skipped.push((index, Skip::Exists));
            continue;
        }
        if !slot.is_empty() {
            if !overwrite || slot.iter().any(|e| e.locked || e.id.is_none()) {
                let skip = if slot.iter().any(|e| e.locked) {
                    Skip::Locked
                } else {
                    Skip::OtherShift
                };
                plan.skipped.push((index, skip));
                continue;
            }
            plan.delete.extend(slot.drain(..).filter_map(|e| e.id));
        }
        slot.push(Existing {
            id: None,
            shift_uid: c.shift_uid.clone(),
            locked: false,
        });
        plan.insert.push(index);
    }
    plan
}

#[cfg(test)]
mod tests {
    use super::*;

    fn cand(email: &str, shift: &str, day: u32) -> Candidate {
        Candidate {
            email_key: email.to_owned(),
            shift_uid: shift.to_owned(),
            date: NaiveDate::from_ymd_opt(2026, 11, day).expect("date"),
        }
    }

    fn existing(email: &str, day: u32, id: i32, shift: &str, locked: bool) -> Occupancy {
        let key = (email.to_owned(), cand(email, shift, day).date);
        HashMap::from([(
            key,
            vec![Existing {
                id: Some(id),
                shift_uid: shift.to_owned(),
                locked,
            }],
        )])
    }

    #[test]
    fn one_shift_per_person_per_day() {
        let items = [
            cand("a", "pagi", 20),
            cand("a", "pagi", 20),
            cand("a", "malam", 20),
            cand("a", "malam", 21),
        ];
        let p = plan(&items, &mut Occupancy::new(), false);
        assert_eq!(p.insert, [0, 3]);
        assert_eq!(p.skipped, [(1, Skip::Exists), (2, Skip::OtherShift)]);
    }

    #[test]
    fn overwrite_replaces_only_unlocked_stored_rows() {
        let items = [cand("a", "malam", 20)];
        let p = plan(&items, &mut existing("a", 20, 7, "pagi", false), false);
        assert_eq!(p.skipped, [(0, Skip::OtherShift)]);

        let p = plan(&items, &mut existing("a", 20, 7, "pagi", false), true);
        assert_eq!((p.insert, p.delete), (vec![0], vec![7]));

        let p = plan(&items, &mut existing("a", 20, 7, "pagi", true), true);
        assert_eq!(p.skipped, [(0, Skip::Locked)]);

        let p = plan(&items, &mut existing("a", 20, 7, "malam", true), true);
        assert_eq!(p.skipped, [(0, Skip::Exists)]);
    }
}
