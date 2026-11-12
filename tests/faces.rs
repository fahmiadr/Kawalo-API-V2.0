//! `/faces` against the scratch services with the fake engine from `common::faces`. Each test
//! uses its own group and emails.

mod common;

use axum::http::{StatusCode, header};
use common::Part;
use common::TestApp;
use common::faces::{
    self, BLACK, BLUE, GREEN, GREY, RED, face_jpeg, join, register, token_with_role, unique,
};
use serde_json::Value;

async fn status(app: &TestApp, token: &str) -> Value {
    let (code, body) = app
        .call("GET", "/api/v1/faces/status", Some(token), None)
        .await;
    assert_eq!(code, StatusCode::OK, "{body}");
    body["data"].clone()
}

async fn audit_count(app: &TestApp, action: &str, target: &str) -> i64 {
    sqlx::query_scalar("SELECT COUNT(*) FROM tkfaceaudit WHERE Action = ? AND TargetEmail = ?")
        .bind(action)
        .bind(target)
        .fetch_one(&app.state.db)
        .await
        .expect("count audit")
}

/// Every stored file of the photo directory, recursively.
fn stored_files(dir: &std::path::Path) -> Vec<Vec<u8>> {
    let mut out = Vec::new();
    for entry in std::fs::read_dir(dir).expect("read dir").flatten() {
        let path = entry.path();
        if path.is_dir() {
            out.extend(stored_files(&path));
        } else {
            out.push(std::fs::read(&path).expect("read file"));
        }
    }
    out
}

#[tokio::test]
async fn requires_a_token() {
    let (app, _dir) = faces::start().await;
    let (code, body) = app.call("GET", "/api/v1/faces/status", None, None).await;
    assert_eq!(code, StatusCode::UNAUTHORIZED);
    assert_eq!(body["error_code"], "AUTH_UNAUTHORIZED");
}

#[tokio::test]
async fn without_an_engine_everything_is_refused() {
    let (app, _dir) = faces::start_unavailable().await;
    let tag = unique();
    let token = app
        .token_for(&format!("a.{tag}@test.id"), &format!("g{tag}"), false)
        .await;

    let data = status(&app, &token).await;
    assert_eq!(data["available"], false);
    assert_eq!(data["registered"], false);

    let (code, body) = register(&app, &token, RED).await;
    assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error_code"], "FACE_SERVICE_UNAVAILABLE");

    let (code, body) = app
        .call_multipart(
            "/api/v1/attendance",
            Some(&token),
            &[
                Part::File("photo", face_jpeg(RED)),
                Part::Text("latitude", "-6.2"),
                Part::Text("longitude", "106.8"),
                Part::Text("nonce", "x"),
            ],
        )
        .await;
    assert_eq!(code, StatusCode::SERVICE_UNAVAILABLE, "{body}");
    assert_eq!(body["error_code"], "FACE_SERVICE_UNAVAILABLE");
}

#[tokio::test]
async fn register_stores_encrypted_data_once() {
    let (app, dir) = faces::start().await;
    let tag = unique();
    let email = format!("a.{tag}@test.id");
    let token = app.token_for(&email, &format!("g{tag}"), false).await;

    let data = status(&app, &token).await;
    assert_eq!(
        (&data["available"], &data["registered"]),
        (&Value::Bool(true), &Value::Bool(false))
    );

    let (code, body) = register(&app, &token, RED).await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    assert_eq!(body["message"], "Wajah berhasil didaftarkan");
    assert_eq!(body["data"]["photo_count"], 2);
    assert_eq!(body["data"]["rejected"], serde_json::json!([]));

    let data = status(&app, &token).await;
    assert_eq!(data["registered"], true);
    assert_eq!(data["photo_count"], 2);
    assert_eq!(data["needs_reregistration"], false);

    // Neither the embedding nor the photos are stored in the clear.
    let files = stored_files(dir.path());
    assert_eq!(files.len(), 2);
    assert!(files.iter().all(|f| !f.starts_with(&[0xFF, 0xD8])));
    let blob: Vec<u8> =
        sqlx::query_scalar("SELECT Embedding FROM tkfaceembedding WHERE UserEmail = ?")
            .bind(&email)
            .fetch_one(&app.state.db)
            .await
            .expect("embedding row");
    assert_eq!(blob.len(), 12 + 8 * 4 + 16);
    assert_eq!(audit_count(&app, "REGISTER", &email).await, 1);

    let (code, body) = register(&app, &token, RED).await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error_code"], "FACE_ALREADY_REGISTERED");
}

#[tokio::test]
async fn poor_photos_are_dropped_and_counted() {
    let (app, _dir) = faces::start().await;
    let tag = unique();
    let token = app
        .token_for(&format!("a.{tag}@test.id"), &format!("g{tag}"), false)
        .await;

    let (code, body) = app
        .call_multipart(
            "/api/v1/faces/register",
            Some(&token),
            &[
                Part::File("photos", face_jpeg(GREY)),
                Part::File("photos", face_jpeg(BLACK)),
                Part::File("photos", face_jpeg(RED)),
            ],
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error_code"], "FACE_QUALITY_LOW");
    assert!(
        body["message"]
            .as_str()
            .is_some_and(|m| m.contains("Foto 1: Wajah terlalu jauh")
                && m.contains("Foto 2: Wajah tidak terdeteksi")),
        "{body}"
    );

    let (code, body) = app
        .call_multipart(
            "/api/v1/faces/register",
            Some(&token),
            &[
                Part::File("photos", face_jpeg(RED)),
                Part::File("photos", face_jpeg(GREY)),
                Part::File("photos", face_jpeg(RED)),
            ],
        )
        .await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
    assert_eq!(body["data"]["photo_count"], 2);
    assert_eq!(body["data"]["rejected"][0]["index"], 2);

    let (code, body) = app
        .call_multipart("/api/v1/faces/register", Some(&token), &[])
        .await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
}

#[tokio::test]
async fn photos_of_different_people_are_refused() {
    let (app, _dir) = faces::start().await;
    let tag = unique();
    let token = app
        .token_for(&format!("a.{tag}@test.id"), &format!("g{tag}"), false)
        .await;
    let (code, body) = app
        .call_multipart(
            "/api/v1/faces/register",
            Some(&token),
            &[
                Part::File("photos", face_jpeg(RED)),
                Part::File("photos", face_jpeg(GREEN)),
            ],
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error_code"], "FACE_QUALITY_LOW");

    let (code, body) = app
        .call_multipart("/api/v1/faces/register", Some(&token), &[])
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
    assert_eq!(body["error_code"], "VALIDATION_ERROR");
}

#[tokio::test]
async fn same_face_twice_in_a_group_is_a_duplicate() {
    let (app, _dir) = faces::start().await;
    let tag = unique();
    let group = format!("g{tag}");
    let (a, b, c) = (
        format!("a.{tag}@test.id"),
        format!("b.{tag}@test.id"),
        format!("c.{tag}@test.id"),
    );
    join(&app, &group, &a, "GUARD").await;
    let token_a = app.token_for(&a, &group, false).await;
    assert_eq!(register(&app, &token_a, RED).await.0, StatusCode::CREATED);

    let token_b = app.token_for(&b, &group, false).await;
    let (code, body) = register(&app, &token_b, RED).await;
    assert_eq!(code, StatusCode::CONFLICT, "{body}");
    assert_eq!(body["error_code"], "FACE_DUPLICATE");
    assert_eq!(audit_count(&app, "DUPLICATE_BLOCKED", &b).await, 1);

    // The same face in another group is not checked against this one.
    let other = app.token_for(&c, &format!("x{tag}"), false).await;
    assert_eq!(register(&app, &other, RED).await.0, StatusCode::CREATED);
    let (code, body) = register(&app, &token_b, BLUE).await;
    assert_eq!(code, StatusCode::CREATED, "{body}");
}

#[tokio::test]
async fn delete_mine_removes_rows_and_files() {
    let (app, dir) = faces::start().await;
    let tag = unique();
    let email = format!("a.{tag}@test.id");
    let token = app.token_for(&email, &format!("g{tag}"), false).await;
    assert_eq!(register(&app, &token, RED).await.0, StatusCode::CREATED);
    assert_eq!(stored_files(dir.path()).len(), 2);

    let (code, body) = app
        .call("DELETE", "/api/v1/faces/me", Some(&token), None)
        .await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["deleted"], true);
    assert!(stored_files(dir.path()).is_empty());
    assert_eq!(status(&app, &token).await["registered"], false);
    assert_eq!(audit_count(&app, "DELETE_SELF", &email).await, 1);

    let (code, body) = app
        .call("DELETE", "/api/v1/faces/me", Some(&token), None)
        .await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(body["data"]["deleted"], false);
    // Registering again works after a deletion.
    assert_eq!(register(&app, &token, RED).await.0, StatusCode::CREATED);
}

#[tokio::test]
async fn delete_mine_works_without_an_engine() {
    let (app, _dir) = faces::start_unavailable().await;
    let tag = unique();
    let token = app
        .token_for(&format!("a.{tag}@test.id"), &format!("g{tag}"), false)
        .await;
    let (code, body) = app
        .call("DELETE", "/api/v1/faces/me", Some(&token), None)
        .await;
    assert_eq!(code, StatusCode::OK, "{body}");
}

#[tokio::test]
async fn admins_view_and_delete_photos_of_their_group_only() {
    let (app, dir) = faces::start().await;
    let tag = unique();
    let group = format!("g{tag}");
    let guard = format!("guard.{tag}@test.id");
    join(&app, &group, &guard, "GUARD").await;
    let guard_token = app.token_for(&guard, &group, false).await;
    assert_eq!(
        register(&app, &guard_token, RED).await.0,
        StatusCode::CREATED
    );
    let list = format!("/api/v1/faces/{guard}/photos");

    // A guard, even with allow_admin, is not an admin.
    let (code, _) = app.call("GET", &list, Some(&guard_token), None).await;
    assert_eq!(code, StatusCode::FORBIDDEN);

    let other_admin = app
        .token_for(&format!("adm.{tag}@test.id"), &format!("x{tag}"), true)
        .await;
    let (code, _) = app.call("GET", &list, Some(&other_admin), None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);

    let admin_email = format!("admin.{tag}@test.id");
    let admin = app.token_for(&admin_email, &group, true).await;
    let (code, body) = app.call("GET", &list, Some(&admin), None).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    let photos = body["data"].as_array().expect("photos").clone();
    assert_eq!(photos.len(), 2);
    let url = photos[0]["url"].as_str().expect("url").to_owned();

    let (code, headers, bytes) = app.call_raw("GET", &url, Some(&admin)).await;
    assert_eq!(code, StatusCode::OK);
    assert_eq!(headers[header::CONTENT_TYPE], "image/jpeg");
    assert_eq!(headers[header::CACHE_CONTROL], "private, no-store");
    assert!(bytes.starts_with(&[0xFF, 0xD8]));
    assert_eq!(audit_count(&app, "VIEW_REGISTER_PHOTO", &guard).await, 1);

    let (code, _, _) = app.call_raw("GET", &url, Some(&guard_token)).await;
    assert_eq!(code, StatusCode::FORBIDDEN);

    let superadmin = token_with_role(
        &app,
        &format!("sa.{tag}@test.id"),
        &format!("y{tag}"),
        "SUPER_ADMIN",
    )
    .await;
    // Biometric data is scoped to the active group even for SUPER_ADMIN.
    let (code, _) = app.call("GET", &list, Some(&superadmin), None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);

    let target = format!("/api/v1/faces/{guard}");
    let (code, _) = app.call("DELETE", &target, Some(&other_admin), None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    let (code, body) = app.call("DELETE", &target, Some(&admin), None).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["deleted"], true);
    assert!(stored_files(dir.path()).is_empty());
    assert_eq!(audit_count(&app, "DELETE_BY_ADMIN", &guard).await, 1);

    let (code, _) = app
        .call(
            "GET",
            "/api/v1/faces/not-an-email/photos",
            Some(&admin),
            None,
        )
        .await;
    assert_eq!(code, StatusCode::UNPROCESSABLE_ENTITY);
}

/// Membership rows of other statuses for the same group.
async fn set_status(app: &TestApp, group: &str, email: &str, status: &str) {
    sqlx::query("UPDATE tkusergroup SET Status = ? WHERE GroupUID = ? AND Email = ?")
        .bind(status)
        .bind(group)
        .bind(email)
        .execute(&app.state.db)
        .await
        .expect("set status");
}

#[tokio::test]
async fn only_joined_members_are_viewable_and_pending_ones_never_deletable() {
    let (app, _dir) = faces::start().await;
    let tag = unique();
    let group = format!("g{tag}");
    let guard = format!("guard.{tag}@test.id");
    join(&app, &group, &guard, "GUARD").await;
    let guard_token = app.token_for(&guard, &group, false).await;
    assert_eq!(
        register(&app, &guard_token, RED).await.0,
        StatusCode::CREATED
    );
    let admin = app
        .token_for(&format!("admin.{tag}@test.id"), &group, true)
        .await;
    let list = format!("/api/v1/faces/{guard}/photos");
    let target = format!("/api/v1/faces/{guard}");

    let (code, body) = app.call("GET", &list, Some(&admin), None).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    let url = body["data"][0]["url"].as_str().expect("url").to_owned();

    // Someone who left: no viewing, but their data can still be erased.
    set_status(&app, &group, &guard, "KICKED").await;
    let (code, _) = app.call("GET", &list, Some(&admin), None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    let (code, _, _) = app.call_raw("GET", &url, Some(&admin)).await;
    assert_eq!(code, StatusCode::NOT_FOUND);

    // A pending invitation is not a membership: nothing, not even deletion.
    set_status(&app, &group, &guard, "JOIN_WAIT").await;
    let (code, _) = app.call("GET", &list, Some(&admin), None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    let (code, _) = app.call("DELETE", &target, Some(&admin), None).await;
    assert_eq!(code, StatusCode::NOT_FOUND);
    assert_eq!(status(&app, &guard_token).await["registered"], true);

    set_status(&app, &group, &guard, "KICKED").await;
    let (code, body) = app.call("DELETE", &target, Some(&admin), None).await;
    assert_eq!(code, StatusCode::OK, "{body}");
    assert_eq!(body["data"]["deleted"], true);
    assert_eq!(audit_count(&app, "DELETE_BY_ADMIN", &guard).await, 1);
}
