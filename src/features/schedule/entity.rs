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

/// Admin view of a schedule row. Shift fields are `None` when the shift row no longer exists.
#[derive(Debug, Serialize, PartialEq, Eq)]
pub struct ManagedItem {
    pub id: String,
    pub date: String,
    pub email: String,
    pub name: String,
    pub avatar_url: Option<String>,
    pub is_member: bool,
    pub shift_uid: Option<String>,
    pub shift_name: Option<String>,
    pub shift_color: Option<String>,
    pub start_time: Option<String>,
    pub end_time: Option<String>,
    pub location_name: Option<String>,
    pub checkin: Option<String>,
    pub checkout: Option<String>,
    pub late_mins: i32,
    pub locked: bool,
}
