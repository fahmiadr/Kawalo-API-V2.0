use serde::Serialize;

#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ScheduleItem {
    pub id: String,
    pub date: String,
    pub shift_uid: String,
    pub shift_name: String,
    pub shift_color: String,
    pub start_time: String,
    pub end_time: String,
    pub location_name: Option<String>,
    pub checkin: Option<String>,
    pub checkout: Option<String>,
    pub late_mins: i32,
}
