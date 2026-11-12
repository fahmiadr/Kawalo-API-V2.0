//! `GET /attendance`, `GET /operational` and `GET /groups/members` against the scratch services.
//! Each test works in its own group and with its own emails, so the tests can run in parallel.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::{Value, json};
use sqlx::MySqlPool;

fn unique() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_owned()
}

async fn insert_group(db: &MySqlPool, uid: &str, name: &str) {
    sqlx::query(
        "INSERT INTO tkgroup (GroupUID, GroupName, GroupOwner) VALUES (?, ?, 'owner@test.id')",
    )
    .bind(uid)
    .bind(name)
    .execute(db)
    .await
    .expect("insert group");
}

async fn insert_user(db: &MySqlPool, email: &str, name: &str, pic: Option<&str>, duty: &str) {
    sqlx::query(
        "INSERT INTO tkuser (UserUID, UserEmail, UserName, Password, UserProfilePic, UserDuty) \
         VALUES (?, ?, ?, 'x', ?, ?)",
    )
    .bind(unique())
    .bind(email)
    .bind(name)
    .bind(pic)
    .bind(duty)
    .execute(db)
    .await
    .expect("insert user");
}

/// A token without an active group, which TypeScript answers with an empty list.
async fn token_without_group(app: &TestApp, email: &str) -> String {
    app.token_for(email, "", false).await
}

async fn unauthorized_and_invalid_queries(app: &TestApp, path: &str) {
    let (status, body) = app.call("GET", path, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(body["success"], false);
    assert_eq!(body["error_code"], "AUTH_UNAUTHORIZED");
    assert_eq!(
        body["message"],
        "Akses tidak diizinkan. Token tidak valid atau telah kedaluwarsa."
    );

    let token = app
        .token_for(&format!("{}@test.id", unique()), "g-none", false)
        .await;
    for query in ["limit=101", "limit=-1", "limit=abc", "offset=-1", "search="] {
        let (status, body) = app
            .call("GET", &format!("{path}?{query}"), Some(&token), None)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{query}: {body}");
        assert_eq!(body["error_code"], "VALIDATION_ERROR", "{query}");
        assert_eq!(
            body["message"], "Format data masukan tidak valid",
            "{query}"
        );
    }

    // Validation runs before authentication, like Elysia.
    let (status, _) = app
        .call("GET", &format!("{path}?limit=101"), None, None)
        .await;
    assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
}

mod attendance {
    use super::*;

    struct Absen<'a> {
        group: &'a str,
        email: &'a str,
        name: &'a str,
        at: &'a str,
        kind: &'a str,
        lat: &'a str,
        long: &'a str,
        diff: i32,
        location: &'a str,
        shift_uid: &'a str,
        sh_start: &'a str,
    }

    impl Default for Absen<'_> {
        fn default() -> Self {
            Self {
                group: "",
                email: "N/A",
                name: "N/A",
                at: "2026-09-18 08:00:00",
                kind: "REGISTER",
                lat: "N/A",
                long: "N/A",
                diff: 0,
                location: "N/A",
                shift_uid: "N/A",
                sh_start: "00:00:00",
            }
        }
    }

    async fn insert(db: &MySqlPool, a: &Absen<'_>) -> u64 {
        sqlx::query(
            "INSERT INTO tkabsen (AbsenEmail, AbsenName, AbsenDT, AbsenType, ALat, ALong, ADiffMins, \
             ALocation, AShiftUID, AbsenGroupUID, AShStart) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?)",
        )
        .bind(a.email)
        .bind(a.name)
        .bind(a.at)
        .bind(a.kind)
        .bind(a.lat)
        .bind(a.long)
        .bind(a.diff)
        .bind(a.location)
        .bind(a.shift_uid)
        .bind(a.group)
        .bind(a.sh_start)
        .execute(db)
        .await
        .expect("insert absen")
        .last_insert_id()
    }

    struct Fixture {
        app: TestApp,
        token: String,
        group: String,
        shift_name: String,
        budi: String,
        ids: [u64; 4],
    }

    /// Newest first: `siti` 16:00, `budi` 08:00, then the zero-date row; plus a row of another group.
    async fn fixture() -> Fixture {
        let app = TestApp::start().await;
        let db = &app.state.db;
        let tag = unique();
        let group = format!("g{tag}");
        let other = format!("o{tag}");
        let shift_uid = format!("sh{tag}");
        let shift_name = format!("Shift Pagi {tag}");
        let budi = format!("budi.{tag}@test.id");
        let siti = format!("siti.{tag}@test.id");
        insert_group(db, &group, &format!("Grup {tag}")).await;
        insert_user(db, &budi, "Budi Santoso", Some("budi.jpg"), "ON DUTY").await;
        // A legacy "unset" picture must come out as null under both names.
        insert_user(db, &siti, "Siti", Some("null"), "OFF DUTY").await;
        sqlx::query(
            "INSERT INTO tkshift (ShiftName, ShiftTimeStart, ShiftTimeStart_mm, ShiftTimeStop, \
             ShiftTimeStop_mm, ShiftGroupUID, ShiftColor, ShiftUID) VALUES (?, 7, 5, 15, 30, ?, '#112233', ?)",
        )
        .bind(&shift_name)
        .bind(&group)
        .bind(&shift_uid)
        .execute(db)
        .await
        .expect("insert shift");

        let ids = [
            insert(
                db,
                &Absen {
                    group: &group,
                    email: &budi,
                    name: "dibuang",
                    kind: "CHECK IN",
                    lat: "-6.200000",
                    long: "106.816666",
                    diff: 3,
                    location: "Pos Utama",
                    shift_uid: &shift_uid,
                    ..Absen::default()
                },
            )
            .await,
            insert(
                db,
                &Absen {
                    group: &group,
                    email: &siti,
                    name: "Siti",
                    at: "2026-09-18 16:00:00",
                    kind: "Absen Pulang",
                    lat: "0",
                    sh_start: "07:30:00",
                    ..Absen::default()
                },
            )
            .await,
            insert(
                db,
                &Absen {
                    group: &group,
                    at: "0000-00-00 00:00:00",
                    ..Absen::default()
                },
            )
            .await,
            insert(
                db,
                &Absen {
                    group: &other,
                    email: &budi,
                    at: "2026-09-19 08:00:00",
                    ..Absen::default()
                },
            )
            .await,
        ];
        let token = app.token_for(&budi, &group, false).await;
        Fixture {
            app,
            token,
            group,
            shift_name,
            budi,
            ids,
        }
    }

    async fn get(f: &Fixture, query: &str) -> Vec<Value> {
        let (status, body) = f
            .app
            .call(
                "GET",
                &format!("/api/v1/attendance{query}"),
                Some(&f.token),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{query}: {body}");
        assert_eq!(body["message"], "Daftar data absensi berhasil diambil");
        body["data"].as_array().expect("data array").clone()
    }

    fn ids(items: &[Value]) -> Vec<&str> {
        items
            .iter()
            .map(|i| i["id"].as_str().expect("id"))
            .collect()
    }

    #[tokio::test]
    async fn rejects_missing_tokens_and_invalid_queries() {
        let app = TestApp::start().await;
        unauthorized_and_invalid_queries(&app, "/api/v1/attendance").await;
    }

    #[tokio::test]
    async fn lists_the_group_newest_first_with_sanitised_fields() {
        let f = fixture().await;
        let items = get(&f, "").await;
        let [budi_id, siti_id, zero_id, _] = f.ids.map(|id| id.to_string());
        assert_eq!(ids(&items), [&siti_id, &budi_id, &zero_id]);

        let siti = &items[0];
        assert_eq!(siti["category"], "pulang");
        assert_eq!(siti["user_name"], "Siti");
        assert_eq!(siti["user_profile_pic"], Value::Null);
        assert_eq!(siti["avatar_url"], Value::Null);
        assert_eq!(siti["latitude"], Value::Null);
        assert_eq!(siti["location_name"], Value::Null);
        assert_eq!(siti["shift_time_window"], "07:30:00");
        assert_eq!(siti["attendance_dt"], "2026-09-18T16:00:00.000Z");

        assert_eq!(
            items[1],
            json!({
                "id": budi_id,
                "user_email": f.budi,
                "user_name": "Budi Santoso",
                "user_profile_pic": "budi.jpg",
                "avatar_url": "budi.jpg",
                "user_duty": "ON DUTY",
                "attendance_dt": "2026-09-18T08:00:00.000Z",
                "attendance_type": "CHECK IN",
                "category": "masuk",
                "latitude": "-6.200000",
                "longitude": "106.816666",
                "diff_minutes": 3,
                "early_leave_minutes": 0,
                "face_verified": null,
                "has_photo": false,
                "face_similarity_percent": null,
                "location_name": "Pos Utama",
                "shift_name": f.shift_name,
                "shift_color": "#112233",
                "shift_time_window": "07:05 - 15:30",
                "group_name": items[1]["group_name"],
            })
        );
        assert!(
            items[1]["group_name"]
                .as_str()
                .is_some_and(|n| n.starts_with("Grup "))
        );

        let zero = &items[2];
        assert_eq!(zero["attendance_dt"], Value::Null);
        assert_eq!(zero["user_email"], Value::Null);
        assert_eq!(zero["attendance_type"], "REGISTER");
        assert_eq!(zero["category"], "lainnya");
        assert_eq!(zero["diff_minutes"], 0);
        assert_eq!(zero["shift_time_window"], Value::Null);
    }

    #[tokio::test]
    async fn paginates_and_lifts_the_limit_with_zero() {
        let f = fixture().await;
        let all = ids(&get(&f, "?limit=0").await)
            .into_iter()
            .map(str::to_owned)
            .collect::<Vec<_>>();
        assert_eq!(all.len(), 3);
        let first = get(&f, "?limit=1").await;
        assert_eq!(ids(&first), [all[0].as_str()]);
        let second = get(&f, "?limit=1&offset=1").await;
        assert_eq!(ids(&second), [all[1].as_str()]);
        assert!(get(&f, "?limit=2&offset=3").await.is_empty());
        assert_eq!(get(&f, "?limit=100&offset=1").await.len(), 2);
    }

    #[tokio::test]
    async fn limit_accepts_the_number_forms_of_t_numeric() {
        let f = fixture().await;
        assert_eq!(get(&f, "?limit=0x2").await.len(), 2);
        assert_eq!(get(&f, "?limit=%201%20").await.len(), 1);
        assert_eq!(get(&f, "?limit=1e0").await.len(), 1);
        assert_eq!(get(&f, "?limit=3&limit=1").await.len(), 1);
        assert_eq!(get(&f, "?limit=0&offset=2").await.len(), 3);
    }

    #[tokio::test]
    async fn filters_by_date_range_and_ignores_unparsable_dates() {
        let f = fixture().await;
        let [budi_id, siti_id, zero_id, _] = f.ids.map(|id| id.to_string());
        let ids_of = |items: &[Value]| {
            ids(items)
                .into_iter()
                .map(str::to_owned)
                .collect::<Vec<_>>()
        };

        let after = get(&f, "?start_date=2026-09-18T12:00:00Z").await;
        assert_eq!(ids_of(&after), std::slice::from_ref(&siti_id));
        let before = get(&f, "?end_date=2026-09-18T12:00:00.000Z").await;
        assert_eq!(ids_of(&before), [budi_id.clone(), zero_id.clone()]);
        let between = get(
            &f,
            "?start_date=2026-09-18T07:59:59Z&end_date=2026-09-18T08:00:00Z",
        )
        .await;
        assert_eq!(ids_of(&between), [budi_id]);
        assert_eq!(
            get(&f, "?start_date=garbage&end_date=0000-00-00")
                .await
                .len(),
            3
        );
        assert!(get(&f, "?start_date=2030-01-01").await.is_empty());
    }

    #[tokio::test]
    async fn searches_name_email_type_location_and_shift_name() {
        let f = fixture().await;
        let tag = &f.group[1..];
        let budi_id = f.ids[0].to_string();
        for search in [
            "budi.".to_owned(),
            "CHECK in".to_owned(),
            "pos utama".to_owned(),
            format!("pagi {tag}"),
            "Siti".to_owned(),
        ] {
            let items = get(&f, &format!("?search={}", search.replace(' ', "%20"))).await;
            assert!(!items.is_empty(), "{search}");
            if search != "Siti" {
                assert_eq!(ids(&items), [budi_id.as_str()], "{search}");
            }
        }
        assert!(get(&f, "?search=tidak-ada-yang-cocok").await.is_empty());
        // A dirty marker is not a search term.
        assert_eq!(get(&f, "?search=N%2FA").await.len(), 3);
    }

    #[tokio::test]
    async fn shift_names_of_other_groups_do_not_leak_into_search() {
        let f = fixture().await;
        let db = &f.app.state.db;
        let tag = unique();
        let other_shift = format!("sx{tag}");
        let name = format!("Rahasia {tag}");
        sqlx::query(
            "INSERT INTO tkshift (ShiftName, ShiftTimeStart, ShiftTimeStop, ShiftGroupUID, ShiftColor, ShiftUID) \
             VALUES (?, 1, 2, 'grup-lain', '#fff', ?)",
        )
        .bind(&name)
        .bind(&other_shift)
        .execute(db)
        .await
        .expect("insert foreign shift");
        insert(
            db,
            &Absen {
                group: &f.group,
                shift_uid: &other_shift,
                ..Absen::default()
            },
        )
        .await;
        assert!(
            get(&f, &format!("?search=Rahasia%20{tag}"))
                .await
                .is_empty()
        );
    }

    #[tokio::test]
    async fn out_of_range_shift_hours_fall_back_to_the_log_start() {
        let f = fixture().await;
        let db = &f.app.state.db;
        let tag = unique();
        let shift_uid = format!("sy{tag}");
        sqlx::query(
            "INSERT INTO tkshift (ShiftName, ShiftTimeStart, ShiftTimeStop, ShiftGroupUID, ShiftColor, ShiftUID) \
             VALUES ('Aneh', 7, 24, ?, '#fff', ?)",
        )
        .bind(&f.group)
        .bind(&shift_uid)
        .execute(db)
        .await
        .expect("insert shift");
        let id = insert(
            db,
            &Absen {
                group: &f.group,
                at: "2026-09-20 08:00:00",
                shift_uid: &shift_uid,
                sh_start: "06:45:00",
                ..Absen::default()
            },
        )
        .await;
        let items = get(&f, "?limit=1").await;
        assert_eq!(items[0]["id"], id.to_string());
        assert_eq!(items[0]["shift_name"], "Aneh");
        assert_eq!(items[0]["shift_time_window"], "06:45:00");
    }

    #[tokio::test]
    async fn a_token_without_a_group_gets_an_empty_list() {
        let f = fixture().await;
        let token = token_without_group(&f.app, &f.budi).await;
        let (status, body) = f
            .app
            .call("GET", "/api/v1/attendance", Some(&token), None)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["success"], true);
        assert_eq!(body["data"], json!([]));
    }
}

mod operational {
    use super::*;

    /// The instant at which the process-local clock reads `local`; the endpoint renders filters
    /// back in local time, so tests must not assume a time zone.
    fn local_iso(local: &str) -> String {
        use chrono::{Local, NaiveDateTime, SecondsFormat, TimeZone, Utc};
        let naive = NaiveDateTime::parse_from_str(local, "%Y-%m-%d %H:%M:%S").expect("local time");
        Local
            .from_local_datetime(&naive)
            .earliest()
            .expect("unambiguous local time")
            .with_timezone(&Utc)
            .to_rfc3339_opts(SecondsFormat::Secs, true)
    }

    struct Journey<'a> {
        group: &'a str,
        email: &'a str,
        name: &'a str,
        duty: &'a str,
        state: &'a str,
        at: &'a str,
        state_at: &'a str,
    }

    async fn insert(db: &MySqlPool, j: &Journey<'_>) -> u64 {
        sqlx::query(
            "INSERT INTO tkdutyjourney (DutyJourneyUID, DutyGroupUID, DutyGroupName, DutyDuty, DutyState, \
             DutyDutyDT, DutyStateDT, UserEmail, UserName) VALUES (?, ?, 'Tim A', ?, ?, ?, ?, ?, ?)",
        )
        .bind(format!("JRN-{}", unique()))
        .bind(j.group)
        .bind(j.duty)
        .bind(j.state)
        .bind(j.at)
        .bind(j.state_at)
        .bind(j.email)
        .bind(j.name)
        .execute(db)
        .await
        .expect("insert journey")
        .last_insert_id()
    }

    struct Fixture {
        app: TestApp,
        token: String,
        budi: String,
        siti: String,
        budi_ids: [u64; 3],
    }

    /// `budi` has two entries on 09-18 and one on 09-17; `siti` has one on 09-18 at 12:00.
    async fn fixture() -> Fixture {
        let app = TestApp::start().await;
        let db = &app.state.db;
        let tag = unique();
        let group = format!("g{tag}");
        let budi = format!("budi.{tag}@test.id");
        let siti = format!("siti.{tag}@test.id");
        insert_user(db, &budi, "Budi Santoso", Some("budi.jpg"), "ON DUTY").await;
        let zero = "0000-00-00 00:00:00";
        let entry = |email, name, duty, state, at, state_at| Journey {
            group: &group,
            email,
            name,
            duty,
            state,
            at,
            state_at,
        };
        let budi_ids = [
            insert(
                db,
                &entry(
                    &budi,
                    "N/A",
                    "Patroli",
                    "Selesai",
                    "2026-09-18 10:00:00",
                    zero,
                ),
            )
            .await,
            insert(
                db,
                &entry(
                    &budi,
                    "Budi",
                    "N/A",
                    "Mulai",
                    "2026-09-18 09:00:00",
                    "2026-09-18 09:30:00",
                ),
            )
            .await,
            insert(
                db,
                &entry(&budi, "Budi", "Jaga", "Mulai", "2026-09-17 20:00:00", zero),
            )
            .await,
        ];
        insert(
            db,
            &entry(&siti, "Siti", "Jaga", "Mulai", "2026-09-18 12:00:00", zero),
        )
        .await;
        // Never listed: other group, blank email, dirty email. The dirty one is the oldest because
        // the database paginates before sanitising, as in TypeScript, so a newer one would eat a page slot.
        for (g, email) in [
            ("lain", budi.as_str()),
            (group.as_str(), ""),
            (group.as_str(), "N/A"),
        ] {
            insert(
                db,
                &Journey {
                    group: g,
                    ..entry(email, "X", "Jaga", "Mulai", "2026-09-16 13:00:00", zero)
                },
            )
            .await;
        }
        let token = app.token_for(&budi, &group, false).await;
        Fixture {
            app,
            token,
            budi,
            siti,
            budi_ids,
        }
    }

    async fn get(f: &Fixture, query: &str) -> Vec<Value> {
        let (status, body) = f
            .app
            .call(
                "GET",
                &format!("/api/v1/operational{query}"),
                Some(&f.token),
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK, "{query}: {body}");
        assert_eq!(
            body["message"],
            "Daftar aktifitas operasional berhasil diambil"
        );
        body["data"].as_array().expect("data array").clone()
    }

    fn card_ids(items: &[Value]) -> Vec<String> {
        items
            .iter()
            .map(|i| i["id"].as_str().expect("id").to_owned())
            .collect()
    }

    #[tokio::test]
    async fn rejects_missing_tokens_and_invalid_queries() {
        let app = TestApp::start().await;
        unauthorized_and_invalid_queries(&app, "/api/v1/operational").await;
    }

    #[tokio::test]
    async fn groups_journeys_into_person_day_cards_newest_first() {
        let f = fixture().await;
        let items = get(&f, "").await;
        assert_eq!(
            card_ids(&items),
            [
                format!("{}_2026-09-18", f.siti),
                format!("{}_2026-09-18", f.budi),
                format!("{}_2026-09-17", f.budi),
            ]
        );

        let [first, second, _] = f.budi_ids.map(|id| id.to_string());
        let card = &items[1];
        assert_eq!(card["user_email"], f.budi);
        assert_eq!(card["user_name"], "Budi");
        assert_eq!(card["user_profile_pic"], "budi.jpg");
        assert_eq!(card["group_name"], "Tim A");
        assert_eq!(card["latest_duty"], "Patroli");
        assert_eq!(card["latest_state"], "Selesai");
        assert_eq!(card["latest_duty_dt"], "2026-09-18T10:00:00.000Z");
        assert_eq!(card["latest_state_dt"], Value::Null);
        assert_eq!(card["total_activities"], 2);
        assert_eq!(card["duty_date"], "2026-09-18");
        let timeline = card["timeline"].as_array().expect("timeline");
        assert_eq!(timeline.len(), 2);
        assert_eq!(timeline[0]["id"], first);
        assert!(
            timeline[0]["journey_uid"]
                .as_str()
                .is_some_and(|u| u.starts_with("JRN-"))
        );
        assert_eq!(timeline[0]["state_dt"], Value::Null);
        assert_eq!(timeline[1]["id"], second);
        assert_eq!(timeline[1]["duty"], Value::Null);
        assert_eq!(timeline[1]["state_dt"], "2026-09-18T09:30:00.000Z");

        assert_eq!(items[0]["user_profile_pic"], Value::Null);
        assert_eq!(items[0]["user_name"], "Siti");
    }

    #[tokio::test]
    async fn paginates_cards_not_rows() {
        let f = fixture().await;
        let all = card_ids(&get(&f, "?limit=0").await);
        assert_eq!(all.len(), 3);
        assert_eq!(card_ids(&get(&f, "?limit=1").await), [all[0].clone()]);
        assert_eq!(
            card_ids(&get(&f, "?limit=1&offset=1").await),
            [all[1].clone()]
        );
        assert_eq!(
            card_ids(&get(&f, "?limit=2&offset=1").await),
            [all[1].clone(), all[2].clone()]
        );
        assert!(get(&f, "?offset=3").await.is_empty());
    }

    #[tokio::test]
    async fn date_filters_pick_cards_but_keep_the_whole_day() {
        let f = fixture().await;
        let from_18 = get(
            &f,
            &format!("?start_date={}", local_iso("2026-09-18 00:00:00")),
        )
        .await;
        assert_eq!(from_18.len(), 2);
        let until_17 = get(
            &f,
            &format!("?end_date={}", local_iso("2026-09-17 23:59:59")),
        )
        .await;
        assert_eq!(card_ids(&until_17), [format!("{}_2026-09-17", f.budi)]);
        assert!(get(&f, "?start_date=2030-01-01T00:00:00Z").await.is_empty());
        assert_eq!(get(&f, "?start_date=garbage").await.len(), 3);

        // 10:00 passes the filter, so budi's whole day is returned, including the 09:00 entry.
        let mid = get(
            &f,
            &format!("?start_date={}", local_iso("2026-09-18 09:30:00")),
        )
        .await;
        let budi_18 = mid
            .iter()
            .find(|i| i["id"] == format!("{}_2026-09-18", f.budi))
            .expect("budi card");
        assert_eq!(budi_18["total_activities"], 2);
    }

    #[tokio::test]
    async fn search_narrows_cards_and_their_timeline() {
        let f = fixture().await;
        let items = get(&f, "?search=Mulai").await;
        let cards = card_ids(&items);
        assert!(cards.contains(&format!("{}_2026-09-18", f.budi)));
        let budi_18 = items
            .iter()
            .find(|i| i["id"] == format!("{}_2026-09-18", f.budi))
            .expect("card");
        assert_eq!(budi_18["total_activities"], 1);
        assert_eq!(budi_18["latest_duty"], Value::Null);
        assert_eq!(budi_18["timeline"].as_array().map(Vec::len), Some(1));

        let by_name = get(&f, "?search=siti").await;
        assert_eq!(card_ids(&by_name), [format!("{}_2026-09-18", f.siti)]);
        assert!(get(&f, "?search=tidak-ada-yang-cocok").await.is_empty());
        assert_eq!(get(&f, "?search=null").await.len(), 3);
    }

    #[tokio::test]
    async fn a_token_without_a_group_gets_an_empty_list() {
        let f = fixture().await;
        let token = token_without_group(&f.app, &f.budi).await;
        let (status, body) = f
            .app
            .call("GET", "/api/v1/operational", Some(&token), None)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(body["data"], json!([]));
    }
}

mod group_members {
    use super::*;

    async fn join(db: &MySqlPool, group: &str, email: &str, status: &str) {
        join_as(db, group, email, status, "USER").await;
    }

    async fn join_as(db: &MySqlPool, group: &str, email: &str, status: &str, category: &str) {
        sqlx::query(
            "INSERT INTO tkusergroup (Email, GroupUID, Status, Category) VALUES (?, ?, ?, ?)",
        )
        .bind(email)
        .bind(group)
        .bind(status)
        .bind(category)
        .execute(db)
        .await
        .expect("insert membership");
    }

    #[tokio::test]
    async fn requires_authentication() {
        let app = TestApp::start().await;
        let (status, body) = app.call("GET", "/api/v1/groups/members", None, None).await;
        assert_eq!(status, StatusCode::UNAUTHORIZED);
        assert_eq!(body["error_code"], "AUTH_UNAUTHORIZED");
        assert_eq!(
            body["message"],
            "Akses tidak diizinkan. Token tidak valid atau telah kedaluwarsa."
        );
    }

    #[tokio::test]
    async fn lists_joined_members_sorted_by_name_or_email() {
        let app = TestApp::start().await;
        let db = &app.state.db;
        let tag = unique();
        let group = format!("g{tag}");
        let email = |who: &str| format!("{who}.{tag}@test.id");
        insert_user(
            db,
            &email("candra"),
            "Candra",
            Some("candra.png"),
            "OFF DUTY",
        )
        .await;
        insert_user(
            db,
            &email("ahmad"),
            "ahmad Zulkifli",
            Some("N/A"),
            "OFF DUTY",
        )
        .await;
        insert_user(db, &email("pending"), "Menunggu", None, "OFF DUTY").await;
        insert_user(db, &email("lain"), "Anggota Lain", None, "OFF DUTY").await;
        insert_user(db, &email("namakosong"), "N/A", None, "OFF DUTY").await;
        for who in ["namakosong", "tanpaprofil"] {
            join(db, &group, &email(who), "JOINED").await;
        }
        join_as(db, &group, &email("candra"), "JOINED", "admin").await;
        join_as(db, &group, &email("ahmad"), "JOINED", "GUARD").await;
        join(db, &group, &email("pending"), "JOIN_WAIT").await;
        join(db, &format!("o{tag}"), &email("lain"), "JOINED").await;

        let token = app.token_for(&email("candra"), &group, false).await;
        let (status, body) = app
            .call("GET", "/api/v1/groups/members", Some(&token), None)
            .await;
        assert_eq!(status, StatusCode::OK, "{body}");
        assert_eq!(body["message"], "Daftar anggota grup berhasil diambil");
        // Names and the emails of nameless members sort as one list, ignoring case first.
        assert_eq!(
            body["data"],
            json!([
                {"email": email("ahmad"), "name": "ahmad Zulkifli", "profilePic": null, "avatar_url": null, "role": "GUARD"},
                {"email": email("candra"), "name": "Candra", "profilePic": "candra.png", "avatar_url": "candra.png", "role": "ADMIN"},
                {"email": email("namakosong"), "name": null, "profilePic": null, "avatar_url": null, "role": "USER"},
                {"email": email("tanpaprofil"), "name": null, "profilePic": null, "avatar_url": null, "role": "USER"},
            ])
        );
    }

    #[tokio::test]
    async fn empty_group_and_missing_group_give_empty_lists() {
        let app = TestApp::start().await;
        let email = format!("{}@test.id", unique());
        for group in [format!("kosong{}", unique()), String::new()] {
            let token = app.token_for(&email, &group, false).await;
            let (status, body) = app
                .call("GET", "/api/v1/groups/members", Some(&token), None)
                .await;
            assert_eq!(status, StatusCode::OK);
            assert_eq!(body["success"], true);
            assert_eq!(body["data"], json!([]));
        }
    }
}
