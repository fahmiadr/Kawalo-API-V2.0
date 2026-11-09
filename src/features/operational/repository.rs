use std::collections::{HashMap, HashSet};

use chrono::{DateTime, Local, Utc};
use sqlx::{FromRow, MySql, MySqlPool, QueryBuilder};

use super::entity::{OperationalItem, TimelineItem};
use crate::core::clean::clean_string;
use crate::features::period_query::{PeriodFilter, clean_timestamp_text};

#[derive(Debug, FromRow)]
struct DutyRow {
    id: i32,
    uid: Option<String>,
    group_name: Option<String>,
    duty: Option<String>,
    state: Option<String>,
    duty_at: Option<String>,
    state_at: Option<String>,
    email: Option<String>,
    name: Option<String>,
    day: Option<String>,
}

/// A person and calendar day, the unit of pagination.
#[derive(Debug, PartialEq, Eq)]
struct Day {
    email: String,
    date: String,
}

impl Day {
    fn key(&self) -> String {
        format!("{}_{}", self.email, self.date)
    }
}

/// `DutyDutyDT` is a `TIMESTAMP`, compared in the session time zone. Filters carry an instant,
/// so render it in the process zone, which TypeScript did too.
fn local_datetime(instant: DateTime<Utc>) -> String {
    instant
        .with_timezone(&Local)
        .format("%Y-%m-%d %H:%M:%S")
        .to_string()
}

fn push_search(query: &mut QueryBuilder<MySql>, pattern: &str) {
    query.push(" AND (");
    for (i, column) in ["DutyDuty", "DutyState", "UserEmail", "UserName"]
        .into_iter()
        .enumerate()
    {
        if i > 0 {
            query.push(" OR ");
        }
        query
            .push(column)
            .push(" LIKE ")
            .push_bind(pattern.to_owned());
    }
    query.push(")");
}

/// Person-day cards of the group, newest first, with the matching journey entries as timeline.
pub async fn list(
    pool: &MySqlPool,
    filter: &PeriodFilter<'_>,
) -> Result<Vec<OperationalItem>, sqlx::Error> {
    let pattern = filter.search.as_ref().map(|s| format!("%{s}%"));

    let mut query = QueryBuilder::<MySql>::new(
        "SELECT UserEmail, DATE_FORMAT(DutyDutyDT, '%Y-%m-%d') AS day FROM tkdutyjourney \
         WHERE DutyGroupUID = ",
    );
    query.push_bind(filter.group_uid);
    if let Some(from) = filter.from {
        query
            .push(" AND DutyDutyDT >= ")
            .push_bind(local_datetime(from));
    }
    if let Some(to) = filter.to {
        query
            .push(" AND DutyDutyDT <= ")
            .push_bind(local_datetime(to));
    }
    if let Some(pattern) = &pattern {
        push_search(&mut query, pattern);
    }
    query.push(
        " AND UserEmail IS NOT NULL AND UserEmail != '' \
         GROUP BY UserEmail, DATE_FORMAT(DutyDutyDT, '%Y-%m-%d') \
         ORDER BY MAX(DutyDutyDT) DESC",
    );
    if let (Some(take), Some(skip)) = (filter.page.take(), filter.page.skip()) {
        query.push(" LIMIT ").push_bind(take);
        query.push(" OFFSET ").push_bind(skip);
    }
    let rows: Vec<(Option<String>, Option<String>)> =
        query.build_query_as().fetch_all(pool).await?;

    let mut seen = HashSet::new();
    let days: Vec<Day> = rows
        .into_iter()
        .filter_map(|(email, date)| {
            Some(Day {
                email: clean_string(email.as_deref())?,
                date: clean_string(date.as_deref())?,
            })
        })
        .filter(|day| seen.insert(day.key()))
        .collect();
    let (Some(first), Some(last)) = (
        days.iter().map(|d| d.date.as_str()).min(),
        days.iter().map(|d| d.date.as_str()).max(),
    ) else {
        return Ok(Vec::new());
    };

    // One scan over the covering date range instead of an OR per card keeps the index usable.
    // Insertion order is kept so the statement text matches the TypeScript one.
    let mut seen_emails = HashSet::new();
    let emails: Vec<&str> = days
        .iter()
        .map(|d| d.email.as_str())
        .filter(|e| seen_emails.insert(*e))
        .collect();
    let (records, pics) = tokio::try_join!(
        fetch_records(pool, filter, &emails, first, last, pattern.as_deref()),
        fetch_pics(pool, &emails),
    )?;
    Ok(assemble(days, records, &pics))
}

async fn fetch_records(
    pool: &MySqlPool,
    filter: &PeriodFilter<'_>,
    emails: &[&str],
    first: &str,
    last: &str,
    pattern: Option<&str>,
) -> Result<Vec<DutyRow>, sqlx::Error> {
    let mut query = QueryBuilder::<MySql>::new(
        "SELECT DutyJourneyId AS id, DutyJourneyUID AS uid, DutyGroupName AS group_name, \
         DutyDuty AS duty, DutyState AS state, CAST(DutyDutyDT AS CHAR) AS duty_at, \
         CAST(DutyStateDT AS CHAR) AS state_at, UserEmail AS email, UserName AS name, \
         DATE_FORMAT(DutyDutyDT, '%Y-%m-%d') AS day \
         FROM tkdutyjourney WHERE DutyGroupUID = ",
    );
    query.push_bind(filter.group_uid);
    query.push(" AND UserEmail IN (");
    let mut list = query.separated(", ");
    for email in emails {
        list.push_bind(*email);
    }
    query
        .push(") AND DutyDutyDT >= ")
        .push_bind(format!("{first} 00:00:00"));
    query
        .push(" AND DutyDutyDT <= ")
        .push_bind(format!("{last} 23:59:59"));
    if let Some(pattern) = pattern {
        push_search(&mut query, pattern);
    }
    query.push(" ORDER BY DutyDutyDT DESC");
    query.build_query_as().fetch_all(pool).await
}

async fn fetch_pics(
    pool: &MySqlPool,
    emails: &[&str],
) -> Result<HashMap<String, Option<String>>, sqlx::Error> {
    let mut query = QueryBuilder::<MySql>::new(
        "SELECT UserEmail, UserProfilePic FROM tkuser WHERE UserEmail IN (",
    );
    let mut list = query.separated(", ");
    for email in emails {
        list.push_bind(*email);
    }
    query.push(")");
    let rows: Vec<(Option<String>, Option<String>)> =
        query.build_query_as().fetch_all(pool).await?;
    Ok(rows
        .into_iter()
        .filter_map(|(email, pic)| {
            Some((
                clean_string(email.as_deref())?,
                clean_string(pic.as_deref()),
            ))
        })
        .collect())
}

fn assemble(
    days: Vec<Day>,
    records: Vec<DutyRow>,
    pics: &HashMap<String, Option<String>>,
) -> Vec<OperationalItem> {
    let mut cards: HashMap<String, Vec<DutyRow>> =
        days.iter().map(|d| (d.key(), Vec::new())).collect();
    for record in records {
        let (Some(email), Some(day)) = (
            clean_string(record.email.as_deref()),
            clean_string(record.day.as_deref()),
        ) else {
            continue;
        };
        if let Some(list) = cards.get_mut(&format!("{email}_{day}")) {
            list.push(record);
        }
    }

    days.into_iter()
        .filter_map(|day| {
            let id = day.key();
            let journeys = cards.remove(&id)?;
            let latest = journeys.first()?;
            let first_clean = |pick: fn(&DutyRow) -> &Option<String>| {
                clean_string(pick(latest).as_deref()).or_else(|| {
                    journeys
                        .iter()
                        .find_map(|j| clean_string(pick(j).as_deref()))
                })
            };
            let user_name = first_clean(|j| &j.name);
            let group_name = first_clean(|j| &j.group_name);
            let item = OperationalItem {
                user_profile_pic: pics.get(&day.email).cloned().flatten(),
                user_name,
                group_name,
                latest_duty: clean_string(latest.duty.as_deref()),
                latest_state: clean_string(latest.state.as_deref()),
                latest_duty_dt: clean_timestamp_text(latest.duty_at.as_deref()),
                latest_state_dt: clean_timestamp_text(latest.state_at.as_deref()),
                total_activities: journeys.len(),
                timeline: journeys.iter().filter_map(timeline_item).collect(),
                id,
                user_email: day.email,
                duty_date: day.date,
            };
            Some(item)
        })
        .collect()
}

fn timeline_item(journey: &DutyRow) -> Option<TimelineItem> {
    let journey_uid = clean_string(journey.uid.as_deref());
    let id = if journey.id > 0 {
        journey.id.to_string()
    } else {
        journey_uid.clone()?
    };
    Some(TimelineItem {
        id,
        journey_uid,
        duty: clean_string(journey.duty.as_deref()),
        state: clean_string(journey.state.as_deref()),
        duty_dt: clean_timestamp_text(journey.duty_at.as_deref()),
        state_dt: clean_timestamp_text(journey.state_at.as_deref()),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn row(id: i32, uid: &str, duty: &str, name: &str, at: &str) -> DutyRow {
        DutyRow {
            id,
            uid: Some(uid.to_owned()),
            group_name: Some("N/A".to_owned()),
            duty: Some(duty.to_owned()),
            state: Some("Selesai".to_owned()),
            duty_at: Some(at.to_owned()),
            state_at: Some("0000-00-00 00:00:00".to_owned()),
            email: Some("Satpam1@Kawalo.id".to_owned()),
            name: Some(name.to_owned()),
            day: Some("2026-09-18".to_owned()),
        }
    }

    fn day() -> Day {
        Day {
            email: "satpam1@kawalo.id".to_owned(),
            date: "2026-09-18".to_owned(),
        }
    }

    #[test]
    fn card_uses_the_latest_row_and_falls_back_to_older_ones() {
        let records = vec![
            row(2, "JRN-2", "Patroli", "N/A", "2026-09-18 10:00:00"),
            row(1, "JRN-1", "N/A", "Budi", "2026-09-18 09:00:00"),
        ];
        let pics = HashMap::from([("satpam1@kawalo.id".to_owned(), Some("p.jpg".to_owned()))]);
        let cards = assemble(vec![day()], records, &pics);
        let [card] = cards.as_slice() else {
            panic!("one card expected: {cards:?}");
        };
        assert_eq!(card.id, "satpam1@kawalo.id_2026-09-18");
        assert_eq!(card.user_name.as_deref(), Some("Budi"));
        assert_eq!(card.group_name, None);
        assert_eq!(card.user_profile_pic.as_deref(), Some("p.jpg"));
        assert_eq!(card.latest_duty.as_deref(), Some("Patroli"));
        assert_eq!(
            card.latest_duty_dt.as_deref(),
            Some("2026-09-18T10:00:00.000Z")
        );
        assert_eq!(card.latest_state_dt, None);
        assert_eq!(card.total_activities, 2);
        assert_eq!(card.timeline[1].duty, None);
    }

    #[test]
    fn rows_without_an_id_leave_the_timeline_but_still_count() {
        let mut anonymous = row(0, "N/A", "Patroli", "Budi", "2026-09-18 11:00:00");
        anonymous.uid = Some(" ".to_owned());
        let kept = row(0, "JRN-9", "Patroli", "Budi", "2026-09-18 10:00:00");
        let cards = assemble(vec![day()], vec![anonymous, kept], &HashMap::new());
        assert_eq!(cards[0].total_activities, 2);
        let ids: Vec<&str> = cards[0].timeline.iter().map(|t| t.id.as_str()).collect();
        assert_eq!(ids, ["JRN-9"]);
    }

    #[test]
    fn days_without_records_are_skipped() {
        assert!(assemble(vec![day()], Vec::new(), &HashMap::new()).is_empty());
    }
}
