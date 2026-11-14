//! `GET /api/v1/ptt/members` against real WebSocket clients. Needs the scratch services from
//! `scripts/dev-services.sh start`.

mod common;
mod ptt_support;

use std::time::Duration;

use axum::http::StatusCode;
use common::TestApp;
use ptt_support::*;
use serde_json::{Value, json};

async fn members(app: &TestApp, token: &str) -> Value {
    let (status, json) = app
        .call("GET", "/api/v1/ptt/members", Some(token), None)
        .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["message"], "Anggota saluran PTT berhasil diambil");
    json["data"]["members"].clone()
}

async fn members_len(app: &TestApp, token: &str) -> usize {
    members(app, token).await.as_array().expect("array").len()
}

fn member(squad: &Squad, who: &str, role: &str, speaking: bool) -> Value {
    json!({
        "user_id": format!("{}@kawalo.test", squad.user(who)),
        "name": squad.user(who),
        "role": role,
        "avatar_url": null,
        "speaking": speaking,
    })
}

#[tokio::test]
async fn lists_connected_members_and_marks_the_floor_holder() {
    let app = TestApp::start().await;
    let addr = app.serve().await;
    let squad = Squad::new();
    let (budi, adi) = (
        squad.token(&app, "budi", false).await,
        squad.token(&app, "adi", true).await,
    );
    assert_eq!(members_len(&app, &budi).await, 0);

    let mut budi_ws = join(addr, &squad.group, &budi).await;
    budi_ws.json().await;
    let mut adi_ws = join(addr, &squad.group, &adi).await;
    adi_ws.json().await;
    budi_ws.json().await;

    // Sorted by name: adi before budi.
    assert_eq!(
        members(&app, &budi).await,
        json!([
            member(&squad, "adi", "ADMIN", false),
            member(&squad, "budi", "GUARD", false)
        ])
    );

    budi_ws
        .send(&json!({"type": "FLOOR_REQUEST", "requestId": "r1"}))
        .await;
    assert_eq!(budi_ws.json().await["type"], "FLOOR_GRANTED");
    assert_eq!(
        members(&app, &adi).await,
        json!([
            member(&squad, "adi", "ADMIN", false),
            member(&squad, "budi", "GUARD", true)
        ])
    );
}

#[tokio::test]
async fn a_member_disappears_when_its_socket_closes() {
    let app = TestApp::start().await;
    let addr = app.serve().await;
    let squad = Squad::new();
    let (budi, sari) = (
        squad.token(&app, "budi", false).await,
        squad.token(&app, "sari", false).await,
    );
    let mut budi_ws = join(addr, &squad.group, &budi).await;
    budi_ws.json().await;
    let mut sari_ws = join(addr, &squad.group, &sari).await;
    sari_ws.json().await;
    assert_eq!(members_len(&app, &budi).await, 2);

    sari_ws.close().await;
    for _ in 0..50 {
        if members_len(&app, &budi).await == 1 {
            break;
        }
        tokio::time::sleep(Duration::from_millis(100)).await;
    }
    assert_eq!(
        members(&app, &budi).await,
        json!([member(&squad, "budi", "GUARD", false)])
    );
}

#[tokio::test]
async fn other_groups_are_not_visible() {
    let app = TestApp::start().await;
    let addr = app.serve().await;
    let (mine, theirs) = (Squad::new(), Squad::new());
    let me = mine.token(&app, "budi", false).await;
    let other = theirs.token(&app, "sari", false).await;
    let mut other_ws = join(addr, &theirs.group, &other).await;
    other_ws.json().await;

    assert_eq!(members_len(&app, &other).await, 1);
    assert_eq!(members(&app, &me).await, json!([]));
}

#[tokio::test]
async fn needs_a_token() {
    let app = TestApp::start().await;
    let (status, json) = app.call("GET", "/api/v1/ptt/members", None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{json}");
}

#[tokio::test]
async fn a_member_carries_the_profile_photo_of_the_user_row() {
    let app = TestApp::start().await;
    let addr = app.serve().await;
    let squad = Squad::new();
    let id = sqlx::query(
        "INSERT INTO tkuser (UserUID, UserEmail, UserName, Password, UserProfilePic) \
         VALUES (?, ?, 'Sari', 'x', 'http://h/uploads/sari.webp')",
    )
    .bind(format!("uid-{}", squad.tag))
    .bind(format!("sari-{}@kawalo.test", squad.tag))
    .execute(&app.state.db)
    .await
    .expect("insert user")
    .last_insert_id()
    .to_string();
    let sari = custom_token(&app, &id, "GUARD", Some(&squad.group), 0).await;
    // A speaker whose id is not a user row (like the other tests) simply has no photo.
    let budi = squad.token(&app, "budi", false).await;
    let mut sari_ws = join(addr, &squad.group, &sari).await;
    sari_ws.json().await;
    let mut budi_ws = join(addr, &squad.group, &budi).await;
    budi_ws.json().await;

    let list = members(&app, &budi).await;
    let photo = |name: &str| {
        list.as_array()
            .expect("array")
            .iter()
            .find(|m| m["name"] == name)
            .expect("member")["avatar_url"]
            .clone()
    };
    assert_eq!(photo(&id), json!("http://h/uploads/sari.webp"));
    assert_eq!(photo(&squad.user("budi")), Value::Null);
}
