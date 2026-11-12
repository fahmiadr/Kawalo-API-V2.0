use serde::Serialize;

use crate::core::clean::clean_string;

/// Response item; every optional field is already sanitised.
#[derive(Debug, Serialize, PartialEq)]
pub struct AttendanceItem {
    pub id: String,
    pub user_email: Option<String>,
    pub user_name: Option<String>,
    pub user_profile_pic: Option<String>,
    /// Same value as `user_profile_pic`, under the name the rest of the API uses.
    pub avatar_url: Option<String>,
    pub user_duty: Option<String>,
    pub attendance_dt: Option<String>,
    pub attendance_type: Option<String>,
    pub category: Category,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub diff_minutes: i64,
    pub early_leave_minutes: i64,
    /// `null` for rows that predate face verification; `false` for a manual row an admin approved.
    pub face_verified: Option<bool>,
    pub has_photo: bool,
    /// Stored match score as a whole percent; only for ADMIN/SUPER_ADMIN callers, else `null`.
    pub face_similarity_percent: Option<i32>,
    pub location_name: Option<String>,
    pub shift_name: Option<String>,
    pub shift_color: Option<String>,
    pub shift_time_window: Option<String>,
    pub group_name: Option<String>,
}

#[derive(Debug, Clone, Copy, Serialize, PartialEq, Eq)]
#[serde(rename_all = "lowercase")]
pub enum Category {
    Masuk,
    Pulang,
    Lainnya,
}

impl Category {
    pub fn of(attendance_type: Option<&str>) -> Self {
        let Some(kind) = attendance_type else {
            return Self::Lainnya;
        };
        let kind = kind.to_lowercase();
        if kind.contains("check in") || kind.contains("masuk") {
            Self::Masuk
        } else if kind.contains("check out") || kind.contains("pulang") {
            Self::Pulang
        } else {
            Self::Lainnya
        }
    }
}

/// `HH:MM - HH:MM` from the shift definition, else the raw `AShStart` unless it is the
/// `00:00:00` column default.
///
/// The minute parts are deliberately not range-checked, as in the TypeScript original.
pub fn shift_time_window(
    shift_hours: Option<(i64, i64)>,
    start_minute: i64,
    stop_minute: i64,
    fallback_start: Option<&str>,
) -> Option<String> {
    if let Some((start, stop)) = shift_hours
        && (0..=23).contains(&start)
        && (0..=23).contains(&stop)
    {
        return Some(format!(
            "{start:02}:{start_minute:02} - {stop:02}:{stop_minute:02}"
        ));
    }
    clean_string(fallback_start).filter(|s| s != "00:00:00")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn category_follows_the_attendance_type() {
        for (kind, want) in [
            (Some("CHECK IN"), Category::Masuk),
            (Some("Absen Masuk"), Category::Masuk),
            (Some("check out"), Category::Pulang),
            (Some("Pulang"), Category::Pulang),
            (Some("REGISTER"), Category::Lainnya),
            (None, Category::Lainnya),
        ] {
            assert_eq!(Category::of(kind), want, "{kind:?}");
        }
    }

    #[test]
    fn time_window_prefers_the_shift_and_falls_back_to_the_log() {
        assert_eq!(
            shift_time_window(Some((7, 15)), 5, 30, Some("08:00:00")).as_deref(),
            Some("07:05 - 15:30")
        );
        assert_eq!(
            shift_time_window(Some((7, 24)), 0, 0, Some("08:00:00")).as_deref(),
            Some("08:00:00")
        );
        assert_eq!(shift_time_window(None, 0, 0, Some("00:00:00")), None);
        assert_eq!(shift_time_window(None, 0, 0, Some("N/A")), None);
        assert_eq!(shift_time_window(None, 0, 0, None), None);
    }
}
