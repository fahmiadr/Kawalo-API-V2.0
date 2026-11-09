use serde::Serialize;

/// One person's day: the latest journey entry plus the matching timeline.
#[derive(Debug, Serialize, PartialEq)]
pub struct OperationalItem {
    pub id: String,
    pub user_email: String,
    pub user_name: Option<String>,
    pub user_profile_pic: Option<String>,
    pub group_name: Option<String>,
    pub latest_duty: Option<String>,
    pub latest_state: Option<String>,
    pub latest_duty_dt: Option<String>,
    pub latest_state_dt: Option<String>,
    /// Counts every matching journey row, including those dropped from `timeline` for lacking an id.
    pub total_activities: usize,
    pub duty_date: String,
    pub timeline: Vec<TimelineItem>,
}

#[derive(Debug, Serialize, PartialEq)]
pub struct TimelineItem {
    pub id: String,
    pub journey_uid: Option<String>,
    pub duty: Option<String>,
    pub state: Option<String>,
    pub duty_dt: Option<String>,
    pub state_dt: Option<String>,
}
