//! `GET`/`PUT /me/home-layout`. Each test uses its own emails and groups.

mod common;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::{Value, json};

const PATH: &str = "/api/v1/me/home-layout";

fn unique() -> String {
    uuid::Uuid::now_v7().simple().to_string()
}

async fn put(app: &TestApp, token: Option<&str>, body: &Value) -> (StatusCode, Value) {
    app.call("PUT", PATH, token, Some(body)).await
}

#[tokio::test]
async fn default_then_saved_layout_per_user_and_group() {
    let app = TestApp::start().await;
    let tag = unique();
    let email = format!("home.{tag}@test.id");
    let (group_a, group_b) = (format!("ga{tag}"), format!("gb{tag}"));
    let token_a = app.token_for(&email, &group_a, false).await;

    let (status, json) = app.call("GET", PATH, Some(&token_a), None).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(
        json["data"],
        json!({"order": ["ptt", "sos", "inc", "pat"], "hidden": [], "is_default": true})
    );

    let body = json!({"order": ["sos", "gst", "ptt", "sch", "inc"], "hidden": ["ptt"]});
    let (status, json) = put(&app, Some(&token_a), &body).await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(json["success"], true);
    let saved = json!({
        "order": ["sos", "gst", "ptt", "sch", "inc"],
        "hidden": ["ptt"],
        "is_default": false
    });
    assert_eq!(json["data"], saved);
    assert_eq!(
        app.call("GET", PATH, Some(&token_a), None).await.1["data"],
        saved
    );

    // Overwrite.
    let (status, json) = put(
        &app,
        Some(&token_a),
        &json!({"order": ["sos"], "hidden": []}),
    )
    .await;
    assert_eq!(status, StatusCode::OK, "{json}");
    assert_eq!(
        app.call("GET", PATH, Some(&token_a), None).await.1["data"],
        json!({"order": ["sos"], "hidden": [], "is_default": false})
    );

    // Same user in another group, and an admin of the first group, still see the default.
    let token_b = app.token_for(&email, &group_b, false).await;
    let other = app
        .token_for(&format!("admin.{tag}@test.id"), &group_a, true)
        .await;
    for token in [&token_b, &other] {
        let (status, json) = app.call("GET", PATH, Some(token), None).await;
        assert_eq!(status, StatusCode::OK, "{json}");
        assert_eq!(json["data"]["is_default"], true);
    }
}

#[tokio::test]
async fn rejects_invalid_layouts_with_422() {
    let app = TestApp::start().await;
    let tag = unique();
    let token = app
        .token_for(&format!("home.{tag}@test.id"), &format!("g{tag}"), false)
        .await;
    let cases = [
        (
            json!({"order": ["sos", "xyz"], "hidden": []}),
            "Kartu 'xyz' tidak dikenal.",
        ),
        (
            json!({"order": ["sos", "ptt"], "hidden": ["abc"]}),
            "Kartu 'abc' tidak dikenal.",
        ),
        (
            json!({"order": ["sos", "ptt", "ptt"], "hidden": []}),
            "Urutan kartu tidak boleh berisi kartu ganda.",
        ),
        (
            json!({"order": ["sos", "ptt"], "hidden": ["ptt", "ptt"]}),
            "Daftar kartu tersembunyi tidak boleh berisi kartu ganda.",
        ),
        (
            json!({"order": ["sos", "ptt"], "hidden": ["inc"]}),
            "Kartu yang disembunyikan harus ada di urutan kartu.",
        ),
        (
            json!({"order": ["ptt", "inc"], "hidden": []}),
            "Kartu SOS wajib ada di beranda.",
        ),
        (
            json!({"order": ["sos", "ptt"], "hidden": ["sos"]}),
            "Kartu SOS tidak boleh disembunyikan.",
        ),
        (
            json!({"order": ["ptt", "sos", "inc", "pat", "gst"], "hidden": []}),
            "Jumlah kartu yang tampil harus 1 sampai 4.",
        ),
    ];
    for (body, message) in cases {
        let (status, json) = put(&app, Some(&token), &body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {json}");
        assert_eq!(json["error_code"], "VALIDATION_ERROR", "{body}");
        assert_eq!(json["message"], message, "{body}");
    }
    // Wrong shapes fail the schema.
    for body in [
        json!({"order": ["sos"]}),
        json!({"order": "sos", "hidden": []}),
    ] {
        let (status, json) = put(&app, Some(&token), &body).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}: {json}");
        assert_eq!(json["error_code"], "VALIDATION_ERROR", "{body}");
    }
    // Five cards with one hidden leaves four visible: allowed.
    let body = json!({"order": ["ptt", "sos", "inc", "pat", "gst"], "hidden": ["gst"]});
    assert_eq!(put(&app, Some(&token), &body).await.0, StatusCode::OK);
    // Nothing invalid was stored over it.
    assert_eq!(
        app.call("GET", PATH, Some(&token), None).await.1["data"]["hidden"],
        json!(["gst"])
    );
}

#[tokio::test]
async fn auth_is_required_and_body_is_checked_first() {
    let app = TestApp::start().await;
    let (status, json) = app.call("GET", PATH, None, None).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{json}");
    assert_eq!(json["error_code"], "AUTH_UNAUTHORIZED");

    let valid = json!({"order": ["sos"], "hidden": []});
    let (status, json) = put(&app, None, &valid).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{json}");
    assert_eq!(json["error_code"], "AUTH_UNAUTHORIZED");

    for invalid in [
        json!({"order": ["ptt"], "hidden": []}),
        json!({"order": 1, "hidden": []}),
    ] {
        let (status, json) = put(&app, None, &invalid).await;
        assert_eq!(
            status,
            StatusCode::UNPROCESSABLE_ENTITY,
            "{invalid}: {json}"
        );
    }
}
