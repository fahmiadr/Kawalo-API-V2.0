//! Check-in/out rules, free of I/O.
//!
//! Like the legacy admin `clockInOut`: a schedule is required, the first attendance of a shift is
//! IN, the next is OUT, and lateness is measured from the shift start. Unlike it, a shift can be
//! checked in only once (no IN after OUT), and a shift ending after midnight is closed against the
//! day it started on.

use chrono::{NaiveDate, NaiveDateTime, NaiveTime, TimeDelta};
use serde::Serialize;

/// How long after its scheduled end yesterday's overnight shift can still be checked out.
/// ponytail: fixed; make it configurable if guards routinely check out later than this.
const OVERNIGHT_CHECKOUT_WINDOW: TimeDelta = TimeDelta::hours(12);
const EARTH_RADIUS_M: f64 = 6_371_008.8;

/// One scheduled shift of the caller.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Slot {
    pub date: NaiveDate,
    pub start: NaiveTime,
    pub stop: NaiveTime,
    pub checked_in: bool,
    pub checked_out: bool,
}

impl Slot {
    /// A stop at or before the start means the shift ends the next day.
    fn overnight(&self) -> bool {
        self.stop <= self.start
    }

    pub fn starts_at(&self) -> NaiveDateTime {
        self.date.and_time(self.start)
    }

    pub fn ends_at(&self) -> NaiveDateTime {
        let end = self.date.and_time(self.stop);
        if self.overnight() {
            end + TimeDelta::days(1)
        } else {
            end
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    In,
    Out,
}

impl Kind {
    /// Stored `AbsenType`; matched as check-in/out by both the legacy admin and `GET /attendance`.
    pub fn absen_type(self) -> &'static str {
        match self {
            Self::In => "CHECK IN",
            Self::Out => "CHECK OUT",
        }
    }
}

#[derive(Debug, PartialEq, Eq)]
pub enum Refusal {
    NoSchedule,
    Completed,
}

/// The slot to act on and whether it is IN or OUT. `slots` are today's and yesterday's.
///
/// 1. An open shift (checked in, not out) is checked out first, so nobody holds two check-ins.
///    Yesterday's counts only if it is overnight and within the checkout window.
/// 2. Otherwise the earliest shift not yet checked in: today's, or yesterday's overnight shift
///    while it is still running.
/// 3. Otherwise refused: completed when today had a shift, else no schedule.
pub fn decide(slots: &[Slot], now: NaiveDateTime) -> Result<(usize, Kind), Refusal> {
    let today = now.date();
    let is_today = |s: &Slot| s.date == today;
    let is_last_night = |s: &Slot| Some(s.date) == today.pred_opt() && s.overnight();

    let open = slots.iter().position(|s| {
        s.checked_in
            && !s.checked_out
            && (is_today(s) || (is_last_night(s) && now <= s.ends_at() + OVERNIGHT_CHECKOUT_WINDOW))
    });
    if let Some(i) = open {
        return Ok((i, Kind::Out));
    }
    let pending = slots
        .iter()
        .enumerate()
        .filter(|(_, s)| !s.checked_in && (is_today(s) || (is_last_night(s) && now < s.ends_at())))
        .min_by_key(|(_, s)| s.starts_at());
    if let Some((i, _)) = pending {
        return Ok((i, Kind::In));
    }
    if slots.iter().any(is_today) {
        Err(Refusal::Completed)
    } else {
        Err(Refusal::NoSchedule)
    }
}

/// Whole minutes after the start (rounded down, as the legacy admin); 0 within the tolerance.
pub fn late_minutes(starts_at: NaiveDateTime, now: NaiveDateTime, tolerance: i64) -> i32 {
    past_tolerance((now - starts_at).num_minutes(), tolerance)
}

/// Whole minutes before the end; 0 within the tolerance.
pub fn early_leave_minutes(ends_at: NaiveDateTime, now: NaiveDateTime, tolerance: i64) -> i32 {
    past_tolerance((ends_at - now).num_minutes(), tolerance)
}

/// `(late, early_leave)` minutes of a check-in or check-out of `slot` made at `now`.
pub fn lateness(
    kind: Kind,
    slot: &Slot,
    now: NaiveDateTime,
    late_tolerance: i64,
    early_tolerance: i64,
) -> (i32, i32) {
    match kind {
        Kind::In => (late_minutes(slot.starts_at(), now, late_tolerance), 0),
        Kind::Out => (0, early_leave_minutes(slot.ends_at(), now, early_tolerance)),
    }
}

fn past_tolerance(minutes: i64, tolerance: i64) -> i32 {
    if minutes > tolerance.max(0) {
        i32::try_from(minutes).unwrap_or(i32::MAX)
    } else {
        0
    }
}

/// Result colour for the app: `on_time` is green, the others red.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum Status {
    OnTime,
    Late,
    EarlyLeave,
}

impl Status {
    pub fn of(kind: Kind, late_minutes: i32, early_leave_minutes: i32) -> Self {
        match kind {
            Kind::In if late_minutes > 0 => Self::Late,
            Kind::Out if early_leave_minutes > 0 => Self::EarlyLeave,
            _ => Self::OnTime,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Geofence {
    /// The shift has no usable location or a radius of 0.
    Unrestricted,
    Inside {
        distance_m: f64,
        radius_m: i32,
    },
    Outside {
        distance_m: f64,
        radius_m: i32,
    },
}

/// Checks `point` against the shift location. Coordinates are legacy strings (`N/A` when unset).
pub fn geofence(lat: &str, long: &str, radius_m: i32, point: (f64, f64)) -> Geofence {
    let parse = |raw: &str, limit: f64| {
        raw.trim()
            .parse::<f64>()
            .ok()
            .filter(|v| v.is_finite() && v.abs() <= limit)
    };
    let (Some(lat), Some(long)) = (parse(lat, 90.0), parse(long, 180.0)) else {
        return Geofence::Unrestricted;
    };
    if radius_m <= 0 {
        return Geofence::Unrestricted;
    }
    let distance_m = haversine_m((lat, long), point);
    if distance_m <= f64::from(radius_m) {
        Geofence::Inside {
            distance_m,
            radius_m,
        }
    } else {
        Geofence::Outside {
            distance_m,
            radius_m,
        }
    }
}

/// Great-circle distance in metres between two `(lat, long)` points in degrees.
pub fn haversine_m(a: (f64, f64), b: (f64, f64)) -> f64 {
    let (lat1, lat2) = (a.0.to_radians(), b.0.to_radians());
    let d_lat = lat2 - lat1;
    let d_long = (b.1 - a.1).to_radians();
    let h = (d_lat / 2.0).sin().powi(2) + lat1.cos() * lat2.cos() * (d_long / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_M * h.sqrt().min(1.0).asin()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn at(date: &str, time: &str) -> NaiveDateTime {
        NaiveDateTime::parse_from_str(&format!("{date} {time}"), "%Y-%m-%d %H:%M").expect("time")
    }

    fn slot(date: &str, start: &str, stop: &str, checked_in: bool, checked_out: bool) -> Slot {
        let time = |t: &str| NaiveTime::parse_from_str(t, "%H:%M").expect("time");
        Slot {
            date: NaiveDate::parse_from_str(date, "%Y-%m-%d").expect("date"),
            start: time(start),
            stop: time(stop),
            checked_in,
            checked_out,
        }
    }

    #[test]
    fn day_shift_goes_in_then_out_then_refuses() {
        let now = at("2026-11-12", "07:05");
        let fresh = [slot("2026-11-12", "07:00", "15:00", false, false)];
        assert_eq!(decide(&fresh, now), Ok((0, Kind::In)));
        let open = [slot("2026-11-12", "07:00", "15:00", true, false)];
        assert_eq!(decide(&open, now), Ok((0, Kind::Out)));
        let done = [slot("2026-11-12", "07:00", "15:00", true, true)];
        assert_eq!(decide(&done, now), Err(Refusal::Completed));
        assert_eq!(decide(&[], now), Err(Refusal::NoSchedule));
        let yesterday = [slot("2026-11-11", "07:00", "15:00", false, false)];
        assert_eq!(decide(&yesterday, now), Err(Refusal::NoSchedule));
    }

    #[test]
    fn overnight_shift_closes_on_the_next_day() {
        let night = slot("2026-11-11", "22:00", "06:00", true, false);
        assert_eq!(night.ends_at(), at("2026-11-12", "06:00"));
        assert_eq!(
            decide(std::slice::from_ref(&night), at("2026-11-12", "06:10")),
            Ok((0, Kind::Out))
        );
        // Past the checkout window the forgotten shift is ignored.
        assert_eq!(
            decide(std::slice::from_ref(&night), at("2026-11-12", "18:01")),
            Err(Refusal::NoSchedule)
        );
        // Open night shift first, then today's morning shift.
        let morning = slot("2026-11-12", "06:00", "14:00", false, false);
        let both = [morning.clone(), night];
        assert_eq!(decide(&both, at("2026-11-12", "06:00")), Ok((1, Kind::Out)));
        let closed = [morning, slot("2026-11-11", "22:00", "06:00", true, true)];
        assert_eq!(
            decide(&closed, at("2026-11-12", "06:01")),
            Ok((0, Kind::In))
        );
        // A late check-in after midnight belongs to last night's running shift.
        let missed = [slot("2026-11-11", "22:00", "06:00", false, false)];
        assert_eq!(
            decide(&missed, at("2026-11-12", "00:30")),
            Ok((0, Kind::In))
        );
        assert_eq!(
            decide(&missed, at("2026-11-12", "06:30")),
            Err(Refusal::NoSchedule)
        );
        // A day shift from yesterday is never reopened.
        let day = [slot("2026-11-11", "07:00", "15:00", true, false)];
        assert_eq!(
            decide(&day, at("2026-11-12", "01:00")),
            Err(Refusal::NoSchedule)
        );
    }

    #[test]
    fn earliest_pending_shift_wins() {
        let slots = [
            slot("2026-11-12", "14:00", "22:00", false, false),
            slot("2026-11-12", "06:00", "14:00", false, false),
        ];
        assert_eq!(decide(&slots, at("2026-11-12", "05:50")), Ok((1, Kind::In)));
    }

    #[test]
    fn lateness_and_early_leave_respect_the_tolerance() {
        let start = at("2026-11-12", "07:00");
        assert_eq!(late_minutes(start, at("2026-11-12", "06:50"), 0), 0);
        assert_eq!(late_minutes(start, at("2026-11-12", "07:00"), 0), 0);
        assert_eq!(late_minutes(start, at("2026-11-12", "07:12"), 0), 12);
        assert_eq!(late_minutes(start, at("2026-11-12", "07:05"), 5), 0);
        assert_eq!(late_minutes(start, at("2026-11-12", "07:06"), 5), 6);
        assert_eq!(late_minutes(start, at("2026-11-12", "07:06"), -3), 6);
        let end = at("2026-11-12", "15:00");
        assert_eq!(early_leave_minutes(end, at("2026-11-12", "15:30"), 0), 0);
        assert_eq!(early_leave_minutes(end, at("2026-11-12", "14:20"), 0), 40);
        assert_eq!(early_leave_minutes(end, at("2026-11-12", "14:50"), 10), 0);
    }

    #[test]
    fn status_maps_to_the_result_colour() {
        assert_eq!(Status::of(Kind::In, 0, 0), Status::OnTime);
        assert_eq!(Status::of(Kind::In, 3, 0), Status::Late);
        assert_eq!(Status::of(Kind::Out, 0, 0), Status::OnTime);
        assert_eq!(Status::of(Kind::Out, 0, 9), Status::EarlyLeave);
    }

    #[test]
    fn haversine_matches_known_distances() {
        // Monas to Bundaran HI, Jakarta: about 2.2 km.
        let d = haversine_m((-6.175392, 106.827153), (-6.194946, 106.823060));
        assert!((d - 2_220.0).abs() < 50.0, "{d}");
        assert!(haversine_m((0.0, 0.0), (0.0, 0.0)).abs() < 1e-9);
        // One degree of latitude is about 111.2 km.
        assert!((haversine_m((0.0, 0.0), (1.0, 0.0)) - 111_195.0).abs() < 10.0);
    }

    #[test]
    fn geofence_needs_a_location_and_a_radius() {
        let here = (-6.2, 106.8);
        assert_eq!(geofence("N/A", "N/A", 100, here), Geofence::Unrestricted);
        assert_eq!(geofence("-6.2", "106.8", 0, here), Geofence::Unrestricted);
        assert_eq!(geofence("999", "106.8", 100, here), Geofence::Unrestricted);
        assert!(matches!(
            geofence("-6.2", "106.8", 50, (-6.2003, 106.8)),
            Geofence::Inside { radius_m: 50, .. }
        ));
        assert!(matches!(
            geofence("-6.2", "106.8", 50, (-6.201, 106.8)),
            Geofence::Outside { radius_m: 50, .. }
        ));
    }
}
