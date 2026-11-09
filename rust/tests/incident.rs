//! Integration tests for `/incidents`; they need the scratch MariaDB and Redis (see
//! `rust/scripts/dev-services.sh`). Every test works in its own random group, so they run in
//! parallel.

mod common;

use std::collections::HashMap;
use std::net::SocketAddr;
use std::time::Duration;

use axum::http::StatusCode;
use common::TestApp;
use serde_json::{Value, json};
use tokio::io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader};
use tokio::net::TcpStream;
use uuid::Uuid;

const UNAUTHORIZED: &str = "Akses tidak diizinkan. Token tidak valid atau telah kedaluwarsa.";
const NO_GROUP: &str = "Pengguna belum memilih grup aktif.";
const NOT_FOUND: &str = "Insiden tidak ditemukan.";

fn unique() -> String {
    Uuid::new_v4().simple().to_string()
}

/// A group with two guards, an admin, and a guard of another group.
struct World {
    app: TestApp,
    group: String,
    suffix: String,
    andi: String,
    budi: String,
    admin: String,
    outsider: String,
}

impl World {
    async fn new() -> Self {
        let app = TestApp::start().await;
        Self::on(app).await
    }

    async fn on(app: TestApp) -> Self {
        let suffix = unique();
        let group = format!("g-{suffix}");
        let andi = app
            .token_for(&format!("andi-{suffix}@x.id"), &group, false)
            .await;
        let budi = app
            .token_for(&format!("budi-{suffix}@x.id"), &group, false)
            .await;
        let admin = app
            .token_for(&format!("admin-{suffix}@x.id"), &group, true)
            .await;
        let outsider = app
            .token_for(
                &format!("citra-{suffix}@x.id"),
                &format!("o-{suffix}"),
                false,
            )
            .await;
        Self {
            app,
            group,
            suffix,
            andi,
            budi,
            admin,
            outsider,
        }
    }

    /// `CreatedAt` has one-second resolution and the list has no tie-breaker (like the TypeScript
    /// query), so tests that care about order spread their rows out explicitly.
    async fn backdate(&self, incident: &Value, seconds_ago: i64) {
        sqlx::query("UPDATE tkincident SET CreatedAt = ? WHERE IncidentUID = ?")
            .bind(chrono::Utc::now() - chrono::Duration::seconds(seconds_ago))
            .bind(incident["id"].as_str().expect("id"))
            .execute(&self.app.state.db)
            .await
            .expect("backdate");
    }

    fn email(&self, who: &str) -> String {
        format!("{who}-{}@x.id", self.suffix)
    }

    async fn call(
        &self,
        method: &str,
        path: &str,
        token: &str,
        body: Option<Value>,
    ) -> (StatusCode, Value) {
        self.app
            .call(method, path, Some(token), body.as_ref())
            .await
    }

    async fn create(&self, token: &str, body: Value) -> Value {
        let (status, json) = self
            .call("POST", "/api/v1/incidents", token, Some(body))
            .await;
        assert_eq!(status, StatusCode::CREATED, "{json}");
        json["data"].clone()
    }

    async fn reply(&self, token: &str, id: &str, body: Value) -> (StatusCode, Value) {
        self.call(
            "POST",
            &format!("/api/v1/incidents/{id}/replies"),
            token,
            Some(body),
        )
        .await
    }
}

fn assert_error(response: &(StatusCode, Value), status: StatusCode, code: &str, message: &str) {
    let (got_status, body) = response;
    assert_eq!(*got_status, status, "{body}");
    assert_eq!(body["success"], json!(false), "{body}");
    assert_eq!(body["error_code"], json!(code), "{body}");
    assert_eq!(body["message"], json!(message), "{body}");
}

fn keys(value: &Value) -> Vec<&str> {
    value
        .as_object()
        .expect("object")
        .keys()
        .map(String::as_str)
        .collect()
}

mod auth_and_validation {
    use super::*;

    #[tokio::test]
    async fn every_endpoint_requires_a_token() {
        let w = World::new().await;
        for (method, path) in [
            ("GET", "/api/v1/incidents"),
            ("GET", "/api/v1/incidents/inc_x"),
            ("GET", "/api/v1/incidents/inc_x/stream"),
            ("PATCH", "/api/v1/incidents/inc_x/resolve"),
            ("DELETE", "/api/v1/incidents/inc_x"),
        ] {
            let response = w.app.call(method, path, None, None).await;
            assert_error(
                &response,
                StatusCode::UNAUTHORIZED,
                "AUTH_UNAUTHORIZED",
                UNAUTHORIZED,
            );
        }
        for (path, body) in [
            ("/api/v1/incidents", json!({"title": "x"})),
            ("/api/v1/incidents/inc_x/replies", json!({"message": "x"})),
        ] {
            let response = w.app.call("POST", path, None, Some(&body)).await;
            assert_error(
                &response,
                StatusCode::UNAUTHORIZED,
                "AUTH_UNAUTHORIZED",
                UNAUTHORIZED,
            );
        }
    }

    #[tokio::test]
    async fn schema_is_validated_before_authentication() {
        let w = World::new().await;
        let (status, body) = w
            .app
            .call(
                "POST",
                "/api/v1/incidents",
                None,
                Some(&json!({"title": ""})),
            )
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(body["error_code"], "VALIDATION_ERROR");
        assert_eq!(body["message"], "Format data masukan tidak valid");
        assert_eq!(
            body["details"],
            "/title: Expected string length greater or equal to 1"
        );

        let long_id = "x".repeat(65);
        for (method, path) in [
            ("GET", format!("/api/v1/incidents/{long_id}")),
            ("GET", format!("/api/v1/incidents/{long_id}/stream")),
            ("PATCH", format!("/api/v1/incidents/{long_id}/resolve")),
            ("DELETE", format!("/api/v1/incidents/{long_id}")),
        ] {
            let (status, body) = w.app.call(method, &path, None, None).await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{path}");
            assert_eq!(
                body["details"],
                "/id: Expected string length less or equal to 64"
            );
        }
        let (status, body) = w
            .app
            .call("GET", "/api/v1/incidents?limit=101", None, None)
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            body["details"],
            "/limit: Expected number to be less or equal to 100"
        );
    }

    #[tokio::test]
    async fn create_and_reply_bodies_follow_the_schema() {
        let w = World::new().await;
        let bad_creates = [
            (json!({}), "/title: Expected required property"),
            (json!({"title": 5}), "/title:"),
            (
                json!({"title": "x".repeat(129)}),
                "/title: Expected string length less or equal to 128",
            ),
            (
                json!({"title": "t", "description": "x".repeat(1025)}),
                "/description: Expected string length less or equal to 1024",
            ),
            (
                json!({"title": "t", "category": "x".repeat(17)}),
                "/category: Expected string length less or equal to 16",
            ),
            (
                json!({"title": "t", "glat": 90.5, "glong": 1}),
                "/glat: Expected number to be less or equal to 90",
            ),
            (
                json!({"title": "t", "glat": -91, "glong": 1}),
                "/glat: Expected number to be greater or equal to -90",
            ),
            (
                json!({"title": "t", "glat": 1, "glong": 181}),
                "/glong: Expected number to be less or equal to 180",
            ),
            (json!({"title": "t", "glat": "1"}), "/glat:"),
            (
                json!({"title": "t", "photos": ["a", "b", "c", "d", "e"]}),
                "/photos: Expected array length less or equal to 4",
            ),
            (
                json!({"title": "t", "photos": ["x".repeat(2049)]}),
                "/photos/0: Expected string length less or equal to 2048",
            ),
            (json!({"title": "t", "photos": "a"}), "/photos:"),
            (json!({"title": "t", "group_id": 7}), "/group_id:"),
        ];
        for (body, detail) in bad_creates {
            let (status, json) = w
                .call("POST", "/api/v1/incidents", &w.andi, Some(body.clone()))
                .await;
            assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY, "{body}");
            assert_eq!(json["error_code"], "VALIDATION_ERROR");
            let details = json["details"].as_str().expect("details");
            assert!(details.starts_with(detail), "{body}: {details}");
        }

        let (status, json) = w
            .reply(&w.andi, "inc_x", json!({"message": "x".repeat(2001)}))
            .await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(
            json["details"],
            "/message: Expected string length less or equal to 2000"
        );
        let (status, json) = w.reply(&w.andi, "inc_x", json!({})).await;
        assert_eq!(status, StatusCode::UNPROCESSABLE_ENTITY);
        assert_eq!(json["details"], "/message: Expected required property");
    }

    #[tokio::test]
    async fn malformed_json_is_a_400() {
        let w = World::new().await;
        let response = w
            .app
            .call("POST", "/api/v1/incidents", Some(&w.andi), None)
            .await;
        assert_eq!(response.0, StatusCode::UNPROCESSABLE_ENTITY);
    }
}

mod create {
    use super::*;

    #[tokio::test]
    async fn returns_the_app_contract_and_takes_the_group_from_the_token() {
        let w = World::new().await;
        let (status, json) = w
            .call(
                "POST",
                "/api/v1/incidents",
                &w.andi,
                Some(json!({
                    "title": "  Pagar rusak ",
                    "group_id": "g-lain",
                    "description": "Sisi barat",
                    "category": "Facility",
                    "glat": -6.2,
                    "glong": 106.8,
                    "photos": ["a.jpg", " ", "N/A", " b.jpg "]
                })),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(json["success"], true);
        assert_eq!(json["message"], "Insiden berhasil dilaporkan");
        let data = &json["data"];
        assert_eq!(
            keys(data),
            [
                "id",
                "group_id",
                "title",
                "description",
                "category",
                "glat",
                "glong",
                "photos",
                "status",
                "created_by",
                "created_by_name",
                "resolved_by",
                "resolved_by_name",
                "resolved_at",
                "deleted_at",
                "created_at",
                "updated_at"
            ]
        );
        assert!(data["id"].as_str().expect("id").starts_with("inc_"));
        assert_eq!(data["group_id"], json!(w.group));
        assert_eq!(data["title"], "Pagar rusak");
        assert_eq!(data["description"], "Sisi barat");
        assert_eq!(data["category"], "facility");
        assert_eq!(
            (&data["glat"], &data["glong"]),
            (&json!(-6.2), &json!(106.8))
        );
        assert_eq!(data["photos"], json!(["a.jpg", "b.jpg"]));
        assert_eq!(data["status"], "OPEN");
        assert_eq!(data["created_by"], json!(w.email("andi")));
        assert_eq!(data["created_by_name"], json!(format!("andi-{}", w.suffix)));
        for null in [
            "resolved_by",
            "resolved_by_name",
            "resolved_at",
            "deleted_at",
        ] {
            assert_eq!(data[null], Value::Null, "{null}");
        }
        let created_at = data["created_at"].as_str().expect("created_at");
        assert!(
            created_at.ends_with(".000Z") && created_at.len() == 24,
            "{created_at}"
        );
        assert_eq!(data["updated_at"], data["created_at"]);

        let (_, detail) = w
            .call(
                "GET",
                &format!("/api/v1/incidents/{}", data["id"].as_str().expect("id")),
                &w.budi,
                None,
            )
            .await;
        assert_eq!(
            detail["data"]["incident"], *data,
            "a re-read equals the create response"
        );
    }

    #[tokio::test]
    async fn defaults_and_nullable_fields() {
        let w = World::new().await;
        let data = w
            .create(
                &w.andi,
                json!({"title": "Minimal", "description": null, "category": null, "glat": null, "glong": null}),
            )
            .await;
        assert_eq!(data["category"], "general");
        assert_eq!(data["photos"], json!([]));
        assert_eq!(
            [&data["description"], &data["glat"], &data["glong"]],
            [&Value::Null, &Value::Null, &Value::Null]
        );
        let whole = w
            .create(
                &w.andi,
                json!({"title": "Bulat", "glat": 5, "glong": -10.0}),
            )
            .await;
        assert_eq!((&whole["glat"], &whole["glong"]), (&json!(5), &json!(-10)));
    }

    #[tokio::test]
    async fn business_rules_are_400_validation_errors() {
        let w = World::new().await;
        let post = |body: Value| {
            let w = &w;
            async move {
                w.call("POST", "/api/v1/incidents", &w.andi, Some(body))
                    .await
            }
        };
        for title in ["   ", "N/A", "null"] {
            assert_error(
                &post(json!({"title": title})).await,
                StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                "Judul insiden wajib diisi.",
            );
        }
        assert_error(
            &post(json!({"title": "t", "category": "ngawur"})).await,
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "Kategori harus salah satu dari: facility, security, hazard, general.",
        );
        for body in [
            json!({"title": "t", "glat": 1}),
            json!({"title": "t", "glong": 1}),
        ] {
            assert_error(
                &post(body).await,
                StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                "Koordinat harus diisi lengkap (glat dan glong).",
            );
        }

        let no_group = {
            let token = w.app.token_for(&w.email("nogroup"), "", false).await;
            w.call(
                "POST",
                "/api/v1/incidents",
                &token,
                Some(json!({"title": "t"})),
            )
            .await
        };
        assert_error(
            &no_group,
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            NO_GROUP,
        );
    }
}

mod list {
    use super::*;

    async fn titles(w: &World, token: &str, query: &str) -> Vec<String> {
        let (status, json) = w
            .call("GET", &format!("/api/v1/incidents{query}"), token, None)
            .await;
        assert_eq!(status, StatusCode::OK, "{json}");
        assert_eq!(json["message"], "Daftar insiden berhasil diambil");
        json["data"]
            .as_array()
            .expect("array")
            .iter()
            .map(|item| item["title"].as_str().expect("title").to_owned())
            .collect()
    }

    #[tokio::test]
    async fn is_scoped_to_the_token_group_and_newest_first() {
        let w = World::new().await;
        for (age, title) in [(30, "satu"), (20, "dua"), (10, "tiga")] {
            let incident = w.create(&w.andi, json!({"title": title})).await;
            w.backdate(&incident, age).await;
        }
        assert_eq!(titles(&w, &w.budi, "").await, ["tiga", "dua", "satu"]);
        assert!(titles(&w, &w.outsider, "").await.is_empty());
        assert_eq!(titles(&w, &w.budi, "?group_id=other").await.len(), 3);
    }

    #[tokio::test]
    async fn a_token_without_a_group_gets_an_empty_list() {
        let w = World::new().await;
        let token = w.app.token_for(&w.email("nogroup"), "", false).await;
        let (status, json) = w.call("GET", "/api/v1/incidents", &token, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["data"], json!([]));
    }

    #[tokio::test]
    async fn filters_by_status_search_and_dates() {
        let w = World::new().await;
        let pagar = w
            .create(
                &w.andi,
                json!({"title": "Pagar rusak", "description": "Sisi koridor B"}),
            )
            .await;
        w.create(&w.budi, json!({"title": "Lampu mati"})).await;
        let (status, _) = w
            .call(
                "PATCH",
                &format!(
                    "/api/v1/incidents/{}/resolve",
                    pagar["id"].as_str().expect("id")
                ),
                &w.budi,
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK);

        assert_eq!(
            titles(&w, &w.andi, "?status=RESOLVED").await,
            ["Pagar rusak"]
        );
        assert_eq!(titles(&w, &w.andi, "?status=open").await, ["Lampu mati"]);
        assert_eq!(titles(&w, &w.andi, "?status=whatever").await.len(), 2);
        assert_eq!(titles(&w, &w.andi, "?search=PAGAR").await, ["Pagar rusak"]);
        assert_eq!(
            titles(&w, &w.andi, "?search=koridor").await,
            ["Pagar rusak"]
        );
        assert_eq!(
            titles(&w, &w.andi, &format!("?search=budi-{}", w.suffix)).await,
            ["Lampu mati"]
        );
        assert!(titles(&w, &w.andi, "?search=tidak-ada").await.is_empty());
        assert!(
            titles(&w, &w.andi, "?search=N/A").await.len() == 2,
            "dirty markers are ignored"
        );

        assert_eq!(
            titles(&w, &w.andi, "?start_date=2000-01-01&end_date=2037-01-01")
                .await
                .len(),
            2
        );
        assert!(
            titles(&w, &w.andi, "?start_date=2999-01-01T00:00:00Z")
                .await
                .is_empty()
        );
        assert!(
            titles(&w, &w.andi, "?end_date=2000-01-01T00:00:00Z")
                .await
                .is_empty()
        );
        assert_eq!(
            titles(&w, &w.andi, "?start_date=bukan-tanggal").await.len(),
            2
        );
    }

    #[tokio::test]
    async fn pages_with_limit_and_offset() {
        let w = World::new().await;
        for n in 0..22 {
            let incident = w
                .create(&w.andi, json!({"title": format!("n{n:02}")}))
                .await;
            w.backdate(&incident, 100 - n).await;
        }
        assert_eq!(titles(&w, &w.andi, "?limit=2").await, ["n21", "n20"]);
        assert_eq!(
            titles(&w, &w.andi, "?limit=2&offset=20").await,
            ["n01", "n00"]
        );
        assert_eq!(
            titles(&w, &w.andi, "?limit=5&offset=2.9")
                .await
                .first()
                .map(String::as_str),
            Some("n19")
        );
        assert_eq!(titles(&w, &w.andi, "").await.len(), 20, "default page size");
        assert_eq!(
            titles(&w, &w.andi, "?limit=0").await.len(),
            20,
            "limit=0 falls back to the default page, as the TypeScript server does"
        );
        assert_eq!(
            titles(&w, &w.andi, "?limit=0&offset=21").await.len(),
            20,
            "and drops the offset"
        );
        assert_eq!(titles(&w, &w.andi, "?limit=100").await.len(), 22);
    }
}

mod detail_and_replies {
    use super::*;

    #[tokio::test]
    async fn detail_returns_the_incident_and_replies_oldest_first() {
        let w = World::new().await;
        let incident = w.create(&w.andi, json!({"title": "Pagar"})).await;
        let id = incident["id"].as_str().expect("id");
        for message in ["pertama", "kedua"] {
            let (status, _) = w.reply(&w.budi, id, json!({"message": message})).await;
            assert_eq!(status, StatusCode::CREATED);
            tokio::time::sleep(Duration::from_millis(5)).await;
        }

        let (status, json) = w
            .call("GET", &format!("/api/v1/incidents/{id}"), &w.andi, None)
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["message"], "Detail insiden berhasil diambil");
        assert_eq!(keys(&json["data"]), ["incident", "replies"]);
        assert_eq!(json["data"]["incident"], incident);
        let replies = json["data"]["replies"].as_array().expect("replies");
        assert_eq!(
            replies
                .iter()
                .map(|r| r["message"].as_str().expect("message"))
                .collect::<Vec<_>>(),
            ["pertama", "kedua"]
        );
        assert_eq!(
            keys(&replies[0]),
            [
                "id",
                "incident_id",
                "sender_id",
                "sender_name",
                "sender_role",
                "message",
                "photos",
                "created_at"
            ]
        );
        assert_eq!(replies[0]["incident_id"], json!(id));
    }

    #[tokio::test]
    async fn other_groups_and_unknown_ids_are_404_and_no_group_is_400() {
        let w = World::new().await;
        let incident = w.create(&w.andi, json!({"title": "Pagar"})).await;
        let id = incident["id"].as_str().expect("id");
        let path = format!("/api/v1/incidents/{id}");
        for token in [&w.outsider] {
            let response = w.call("GET", &path, token, None).await;
            assert_error(
                &response,
                StatusCode::NOT_FOUND,
                "RESOURCE_NOT_FOUND",
                NOT_FOUND,
            );
            let response = w.reply(token, id, json!({"message": "x"})).await;
            assert_error(
                &response,
                StatusCode::NOT_FOUND,
                "RESOURCE_NOT_FOUND",
                NOT_FOUND,
            );
            let response = w
                .call("PATCH", &format!("{path}/resolve"), token, None)
                .await;
            assert_error(
                &response,
                StatusCode::NOT_FOUND,
                "RESOURCE_NOT_FOUND",
                NOT_FOUND,
            );
        }
        let response = w
            .call("GET", "/api/v1/incidents/inc_tidak-ada", &w.andi, None)
            .await;
        assert_error(
            &response,
            StatusCode::NOT_FOUND,
            "RESOURCE_NOT_FOUND",
            NOT_FOUND,
        );

        let no_group = w.app.token_for(&w.email("nogroup"), "", false).await;
        for (method, suffix) in [
            ("GET", ""),
            ("PATCH", "/resolve"),
            ("DELETE", ""),
            ("GET", "/stream"),
        ] {
            let response = w
                .call(method, &format!("{path}{suffix}"), &no_group, None)
                .await;
            assert_error(
                &response,
                StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                NO_GROUP,
            );
        }
        let response = w.reply(&no_group, id, json!({"message": "x"})).await;
        assert_error(
            &response,
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            NO_GROUP,
        );
    }

    #[tokio::test]
    async fn reply_returns_201_and_records_the_sender() {
        let w = World::new().await;
        let incident = w.create(&w.andi, json!({"title": "Pagar"})).await;
        let id = incident["id"].as_str().expect("id");

        let (status, json) = w
            .reply(
                &w.budi,
                id,
                json!({"message": "  Sedang ditangani ", "photos": ["p.jpg"]}),
            )
            .await;
        assert_eq!(status, StatusCode::CREATED);
        assert_eq!(json["message"], "Balasan berhasil dikirim");
        let data = &json["data"];
        assert!(data["id"].as_str().expect("id").starts_with("rpl_"));
        assert_eq!(data["incident_id"], json!(id));
        assert_eq!(data["sender_id"], json!(w.email("budi")));
        assert_eq!(data["sender_name"], json!(format!("budi-{}", w.suffix)));
        assert_eq!(data["sender_role"], "GUARD");
        assert_eq!(data["message"], "Sedang ditangani");
        assert_eq!(data["photos"], json!(["p.jpg"]));

        let (status, json) = w
            .reply(&w.budi, id, json!({"message": "", "photos": ["x.jpg"]}))
            .await;
        assert_eq!(
            status,
            StatusCode::CREATED,
            "a photo alone is enough: {json}"
        );
        assert_eq!(json["data"]["message"], "");
    }

    #[tokio::test]
    async fn reply_rules() {
        let w = World::new().await;
        let incident = w.create(&w.andi, json!({"title": "Pagar"})).await;
        let id = incident["id"].as_str().expect("id");

        for body in [
            json!({"message": ""}),
            json!({"message": "  "}),
            json!({"message": "N/A", "photos": [" "]}),
        ] {
            assert_error(
                &w.reply(&w.andi, id, body).await,
                StatusCode::BAD_REQUEST,
                "VALIDATION_ERROR",
                "Pesan atau foto wajib diisi.",
            );
        }

        let (status, _) = w
            .call(
                "PATCH",
                &format!("/api/v1/incidents/{id}/resolve"),
                &w.andi,
                None,
            )
            .await;
        assert_eq!(status, StatusCode::OK);
        assert_error(
            &w.reply(&w.andi, id, json!({"message": "terlambat"})).await,
            StatusCode::BAD_REQUEST,
            "VALIDATION_ERROR",
            "Insiden sudah diselesaikan, diskusi ditutup.",
        );
    }
}

mod resolve_and_delete {
    use super::*;

    #[tokio::test]
    async fn resolve_is_idempotent_and_keeps_the_first_resolver() {
        let w = World::new().await;
        let incident = w.create(&w.andi, json!({"title": "Pagar"})).await;
        let path = format!(
            "/api/v1/incidents/{}/resolve",
            incident["id"].as_str().expect("id")
        );

        let (status, json) = w.call("PATCH", &path, &w.budi, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(json["message"], "Insiden berhasil diselesaikan");
        let data = &json["data"];
        assert_eq!(data["status"], "RESOLVED");
        assert_eq!(data["resolved_by"], json!(w.email("budi")));
        assert_eq!(
            data["resolved_by_name"],
            json!(format!("budi-{}", w.suffix))
        );
        let resolved_at = data["resolved_at"].as_str().expect("resolved_at");
        assert!(resolved_at.ends_with(".000Z"), "{resolved_at}");
        assert_eq!(data["created_by"], json!(w.email("andi")));

        let (status, again) = w.call("PATCH", &path, &w.andi, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(again["data"], *data, "second resolve changes nothing");
    }

    #[tokio::test]
    async fn only_admins_delete_and_deleted_incidents_vanish() {
        let w = World::new().await;
        let incident = w.create(&w.andi, json!({"title": "Pagar"})).await;
        let id = incident["id"].as_str().expect("id");
        let path = format!("/api/v1/incidents/{id}");

        assert_error(
            &w.call("DELETE", &path, &w.andi, None).await,
            StatusCode::FORBIDDEN,
            "AUTH_FORBIDDEN",
            "Hanya administrator yang dapat menghapus insiden.",
        );
        assert_error(
            &w.call("DELETE", &path, &w.outsider, None).await,
            StatusCode::FORBIDDEN,
            "AUTH_FORBIDDEN",
            "Hanya administrator yang dapat menghapus insiden.",
        );

        let (status, json) = w.call("DELETE", &path, &w.admin, None).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(
            json,
            json!({"success": true, "message": "Insiden berhasil dihapus", "data": null})
        );

        let row: (Option<chrono::DateTime<chrono::Utc>>, String) =
            sqlx::query_as("SELECT DeletedAt, Status FROM tkincident WHERE IncidentUID = ?")
                .bind(id)
                .fetch_one(&w.app.state.db)
                .await
                .expect("row stays");
        assert!(row.0.is_some(), "soft delete sets DeletedAt");

        for (method, suffix) in [
            ("GET", ""),
            ("DELETE", ""),
            ("GET", "/stream"),
            ("PATCH", "/resolve"),
        ] {
            let token = if method == "DELETE" {
                &w.admin
            } else {
                &w.andi
            };
            assert_error(
                &w.call(method, &format!("{path}{suffix}"), token, None)
                    .await,
                StatusCode::NOT_FOUND,
                "RESOURCE_NOT_FOUND",
                NOT_FOUND,
            );
        }
        assert_error(
            &w.reply(&w.andi, id, json!({"message": "x"})).await,
            StatusCode::NOT_FOUND,
            "RESOURCE_NOT_FOUND",
            NOT_FOUND,
        );
        let (_, list) = w.call("GET", "/api/v1/incidents", &w.andi, None).await;
        assert_eq!(list["data"], json!([]));
    }
}

/// Server-sent events over a real socket, read frame by frame.
mod sse {
    use super::*;

    pub struct Client {
        reader: BufReader<TcpStream>,
        status: u16,
        headers: HashMap<String, String>,
        buffer: String,
    }

    impl Client {
        pub async fn connect(
            addr: SocketAddr,
            path: &str,
            token: &str,
            last_event_id: Option<&str>,
        ) -> Self {
            let mut socket = TcpStream::connect(addr).await.expect("connect");
            let mut request = format!(
                "GET {path} HTTP/1.1\r\nHost: localhost\r\nAuthorization: Bearer {token}\r\nAccept: text/event-stream\r\n"
            );
            if let Some(id) = last_event_id {
                request.push_str(&format!("Last-Event-ID: {id}\r\n"));
            }
            request.push_str("\r\n");
            socket.write_all(request.as_bytes()).await.expect("write");

            let mut reader = BufReader::new(socket);
            let mut line = String::new();
            reader.read_line(&mut line).await.expect("status line");
            let status = line
                .split_whitespace()
                .nth(1)
                .expect("status")
                .parse()
                .expect("code");
            let mut headers = HashMap::new();
            loop {
                line.clear();
                reader.read_line(&mut line).await.expect("header");
                let trimmed = line.trim_end();
                if trimmed.is_empty() {
                    break;
                }
                let (name, value) = trimmed.split_once(':').expect("header");
                headers.insert(name.to_ascii_lowercase(), value.trim().to_owned());
            }
            Self {
                reader,
                status,
                headers,
                buffer: String::new(),
            }
        }

        /// Next HTTP chunk of the body, `None` at the terminating chunk or on timeout.
        async fn chunk(&mut self, wait: Duration) -> Option<String> {
            let read = async {
                let mut size = String::new();
                self.reader.read_line(&mut size).await.ok()?;
                let size = usize::from_str_radix(size.trim().split(';').next()?, 16).ok()?;
                let mut data = vec![0; size + 2];
                self.reader.read_exact(&mut data).await.ok()?;
                data.truncate(size);
                (size > 0).then(|| String::from_utf8(data).expect("utf8"))
            };
            tokio::time::timeout(wait, read).await.ok().flatten()
        }

        /// The next complete frame (up to and including the blank line).
        pub async fn frame(&mut self) -> String {
            self.frame_within(Duration::from_secs(5))
                .await
                .expect("a frame in time")
        }

        async fn frame_within(&mut self, wait: Duration) -> Option<String> {
            let deadline = tokio::time::Instant::now() + wait;
            loop {
                if let Some(end) = self.buffer.find("\n\n") {
                    return Some(self.buffer.drain(..end + 2).collect());
                }
                let left = deadline.saturating_duration_since(tokio::time::Instant::now());
                let chunk = self.chunk(left).await?;
                self.buffer.push_str(&chunk);
            }
        }

        async fn expect_silence(&mut self, wait: Duration) {
            if let Some(frame) = self.frame_within(wait).await {
                panic!("unexpected frame {frame:?}");
            }
        }
    }

    async fn open_world() -> (World, SocketAddr, String) {
        let w = World::new().await;
        let addr = w.app.serve().await;
        let incident = w.create(&w.andi, json!({"title": "Pagar rusak"})).await;
        let id = incident["id"].as_str().expect("id").to_owned();
        (w, addr, id)
    }

    fn stream_path(id: &str) -> String {
        format!("/api/v1/incidents/{id}/stream")
    }

    async fn subscribers(w: &World, id: &str) -> i64 {
        let mut redis = w.app.state.redis.clone();
        let counts: HashMap<String, i64> = redis::cmd("PUBSUB")
            .arg("NUMSUB")
            .arg(format!("incident:{id}:events"))
            .query_async(&mut redis)
            .await
            .expect("numsub");
        counts.into_values().next().unwrap_or(0)
    }

    async fn wait_for_subscribers(w: &World, id: &str, expected: i64) {
        for _ in 0..50 {
            if subscribers(w, id).await == expected {
                return;
            }
            tokio::time::sleep(Duration::from_millis(100)).await;
        }
        panic!(
            "expected {expected} Redis subscribers, found {}",
            subscribers(w, id).await
        );
    }

    #[tokio::test]
    async fn opens_with_the_sse_headers_and_a_retry_hint() {
        let (w, addr, id) = open_world().await;
        let mut client = Client::connect(addr, &stream_path(&id), &w.budi, None).await;
        assert_eq!(client.status, 200);
        assert_eq!(
            client.headers["content-type"],
            "text/event-stream;charset=utf-8"
        );
        assert_eq!(client.headers["cache-control"], "no-cache, no-transform");
        assert_eq!(client.headers["connection"], "keep-alive");
        assert_eq!(client.headers["x-accel-buffering"], "no");
        assert_eq!(client.headers["transfer-encoding"], "chunked");
        assert_eq!(
            client.chunk(Duration::from_secs(5)).await.as_deref(),
            Some("retry: 3000\n\n")
        );
        client.expect_silence(Duration::from_millis(300)).await;
    }

    #[tokio::test]
    async fn pushes_replies_resolves_and_deletes_as_exact_frames() {
        let (w, addr, id) = open_world().await;
        let mut client = Client::connect(addr, &stream_path(&id), &w.budi, None).await;
        assert_eq!(client.frame().await, "retry: 3000\n\n");

        let (_, reply) = w
            .reply(
                &w.andi,
                &id,
                json!({"message": "Sedang ditangani \"siap\"", "photos": ["p.jpg"]}),
            )
            .await;
        let reply = &reply["data"];
        let frame = client.frame().await;
        let reply_id = reply["id"].as_str().expect("reply id");
        assert_eq!(
            frame,
            format!(
                "id: {reply_id}\nevent: new_reply\ndata: {}\n\n",
                serde_json::to_string(reply).expect("json")
            ),
            "data is the reply item, compact JSON in the same key order as the HTTP response"
        );

        let (_, resolved) = w
            .call(
                "PATCH",
                &format!("/api/v1/incidents/{id}/resolve"),
                &w.budi,
                None,
            )
            .await;
        let resolved_at = resolved["data"]["resolved_at"]
            .as_str()
            .expect("resolved_at");
        assert_eq!(
            client.frame().await,
            format!(
                "event: thread_resolved\ndata: {{\"incident_id\":\"{id}\",\"resolved_by\":\"{}\",\"resolved_by_name\":\"budi-{}\",\"resolved_at\":\"{resolved_at}\"}}\n\n",
                w.email("budi"),
                w.suffix
            )
        );

        // Idempotent resolve publishes nothing new.
        w.call(
            "PATCH",
            &format!("/api/v1/incidents/{id}/resolve"),
            &w.andi,
            None,
        )
        .await;
        client.expect_silence(Duration::from_millis(300)).await;

        w.call("DELETE", &format!("/api/v1/incidents/{id}"), &w.admin, None)
            .await;
        let frame = client.frame().await;
        let prefix =
            format!("event: incident_deleted\ndata: {{\"incident_id\":\"{id}\",\"deleted_at\":\"");
        assert!(
            frame.starts_with(&prefix) && frame.ends_with("Z\"}\n\n"),
            "{frame}"
        );
        let deleted_at = &frame[prefix.len()..frame.len() - 4];
        assert_eq!(deleted_at.len(), 24, "{deleted_at}");
    }

    #[tokio::test]
    async fn reconnecting_with_last_event_id_asks_for_a_resync() {
        let (w, addr, id) = open_world().await;
        let mut client = Client::connect(addr, &stream_path(&id), &w.budi, Some("rpl_9")).await;
        assert_eq!(client.frame().await, "retry: 3000\n\n");
        assert_eq!(client.frame().await, "event: sync_required\ndata: {}\n\n");
        client.expect_silence(Duration::from_millis(300)).await;
    }

    #[tokio::test]
    async fn refuses_without_access_with_plain_json_errors() {
        let (w, addr, id) = open_world().await;
        for (token, status) in [(&w.outsider, 404), (&"bukan-token".to_owned(), 401)] {
            let client = Client::connect(addr, &stream_path(&id), token, None).await;
            assert_eq!(client.status, status);
            assert!(client.headers["content-type"].starts_with("application/json"));
        }
        let unknown = Client::connect(addr, &stream_path("inc_tidak-ada"), &w.budi, None).await;
        assert_eq!(unknown.status, 404);
    }

    #[tokio::test]
    async fn streams_share_one_subscription_that_is_released_when_clients_leave() {
        let (w, addr, id) = open_world().await;
        assert_eq!(subscribers(&w, &id).await, 0);

        let first = Client::connect(addr, &stream_path(&id), &w.budi, None).await;
        let mut second = Client::connect(addr, &stream_path(&id), &w.andi, None).await;
        assert_eq!(second.frame().await, "retry: 3000\n\n");
        assert_eq!(
            subscribers(&w, &id).await,
            1,
            "one Redis subscription shared by both streams"
        );

        drop(first);
        w.reply(&w.budi, &id, json!({"message": "masih ada"})).await;
        assert!(
            second.frame().await.contains("masih ada"),
            "the remaining stream keeps receiving"
        );
        assert_eq!(subscribers(&w, &id).await, 1);

        drop(second);
        wait_for_subscribers(&w, &id, 0).await;

        // A later stream subscribes again.
        let mut third = Client::connect(addr, &stream_path(&id), &w.budi, None).await;
        assert_eq!(third.frame().await, "retry: 3000\n\n");
        assert_eq!(subscribers(&w, &id).await, 1);
    }

    #[tokio::test]
    async fn another_server_instance_delivers_events_through_redis() {
        let (w, addr, id) = open_world().await;
        let mut client = Client::connect(addr, &stream_path(&id), &w.budi, None).await;
        assert_eq!(client.frame().await, "retry: 3000\n\n");

        let other = World::on(TestApp::start().await).await;
        let token = other.app.token_for(&w.email("andi"), &w.group, false).await;
        let (status, _) = other
            .reply(&token, &id, json!({"message": "dari instans lain"}))
            .await;
        assert_eq!(status, StatusCode::CREATED);
        assert!(client.frame().await.contains("dari instans lain"));
    }

    #[tokio::test]
    async fn typescript_payloads_are_understood_and_garbage_is_ignored() {
        let (w, addr, id) = open_world().await;
        let mut client = Client::connect(addr, &stream_path(&id), &w.budi, None).await;
        assert_eq!(client.frame().await, "retry: 3000\n\n");

        let mut redis = w.app.state.redis.clone();
        let channel = format!("incident:{id}:events");
        for payload in [
            "not json".to_owned(),
            r#"{"type":"unknown"}"#.to_owned(),
            r#"{"type":"thread_resolved","incidentId":"x","resolvedBy":"a","resolvedByName":"A","resolvedAt":"nope"}"#.to_owned(),
            json!({
                "type": "new_reply",
                "reply": {
                    "id": "rpl_ts", "incidentId": id, "senderEmail": "a@x.id", "senderName": "Andi",
                    "senderRole": "GUARD", "message": "dari TypeScript", "photos": [],
                    "createdAt": "2026-01-01T00:00:00.000Z"
                }
            })
            .to_string(),
        ] {
            let _: i64 = redis::cmd("PUBLISH").arg(&channel).arg(payload).query_async(&mut redis).await.expect("publish");
        }
        assert_eq!(
            client.frame().await,
            "id: rpl_ts\nevent: new_reply\ndata: {\"id\":\"rpl_ts\",\"incident_id\":\"".to_owned()
                + &id
                + "\",\"sender_id\":\"a@x.id\",\"sender_name\":\"Andi\",\"sender_role\":\"GUARD\",\"message\":\"dari TypeScript\",\"photos\":[],\"created_at\":\"2026-01-01T00:00:00.000Z\"}\n\n"
        );
    }

    #[tokio::test]
    async fn concurrent_resolves_publish_exactly_one_event() {
        let (w, addr, id) = open_world().await;
        let mut client = Client::connect(addr, &stream_path(&id), &w.budi, None).await;
        assert_eq!(client.frame().await, "retry: 3000\n\n");

        let path = format!("/api/v1/incidents/{id}/resolve");
        let mut tokens = vec![w.andi.clone(), w.budi.clone(), w.admin.clone()];
        for n in 0..5 {
            tokens.push(
                w.app
                    .token_for(&format!("extra{n}-{}@x.id", w.suffix), &w.group, false)
                    .await,
            );
        }
        let results = futures_util::future::join_all(
            tokens
                .iter()
                .map(|token| w.call("PATCH", &path, token, None)),
        )
        .await;
        let resolvers: std::collections::HashSet<_> = results
            .iter()
            .map(|(status, json)| {
                assert_eq!(*status, StatusCode::OK, "{json}");
                json["data"]["resolved_by"]
                    .as_str()
                    .expect("resolved_by")
                    .to_owned()
            })
            .collect();
        assert_eq!(
            resolvers.len(),
            1,
            "everyone sees the same winner: {resolvers:?}"
        );

        assert!(client.frame().await.starts_with("event: thread_resolved\n"));
        client.expect_silence(Duration::from_millis(500)).await;
    }

    #[tokio::test]
    async fn heartbeat_comment_every_fifteen_seconds() {
        let (w, addr, id) = open_world().await;
        let mut client = Client::connect(addr, &stream_path(&id), &w.budi, None).await;
        assert_eq!(client.frame().await, "retry: 3000\n\n");
        client.expect_silence(Duration::from_secs(14)).await;
        let started = tokio::time::Instant::now();
        assert_eq!(
            client.frame_within(Duration::from_secs(3)).await.as_deref(),
            Some(": ping\n\n")
        );
        assert!(started.elapsed() < Duration::from_secs(3));
    }
}

/// Recovery of the shared subscriber connection, on a private Redis so killing connections cannot
/// disturb tests running in parallel.
mod subscriber_recovery {
    use super::*;
    use std::process::{Child, Command, Stdio};

    struct PrivateRedis {
        child: Child,
        url: String,
    }

    impl PrivateRedis {
        async fn start() -> Option<Self> {
            let port = std::net::TcpListener::bind("127.0.0.1:0")
                .ok()?
                .local_addr()
                .ok()?
                .port();
            let child = Command::new("redis-server")
                .args([
                    "--port",
                    &port.to_string(),
                    "--bind",
                    "127.0.0.1",
                    "--save",
                    "",
                    "--appendonly",
                    "no",
                ])
                .stdout(Stdio::null())
                .stderr(Stdio::null())
                .spawn()
                .ok()?;
            let me = Self {
                child,
                url: format!("redis://127.0.0.1:{port}"),
            };
            for _ in 0..50 {
                if let Ok(client) = redis::Client::open(me.url.as_str())
                    && let Ok(mut conn) = client.get_multiplexed_async_connection().await
                    && redis::cmd("PING")
                        .query_async::<String>(&mut conn)
                        .await
                        .is_ok()
                {
                    return Some(me);
                }
                tokio::time::sleep(Duration::from_millis(100)).await;
            }
            None
        }
    }

    impl Drop for PrivateRedis {
        fn drop(&mut self) {
            let _ = self.child.kill();
            let _ = self.child.wait();
        }
    }

    #[tokio::test]
    async fn resubscribes_after_the_connection_is_killed() {
        let redis = PrivateRedis::start().await.expect("redis-server on PATH");
        let url = redis.url.clone();
        let app = TestApp::start_with(move |config| config.redis_url = url).await;
        let w = World::on(app).await;
        let addr = w.app.serve().await;
        let incident = w.create(&w.andi, json!({"title": "Pagar"})).await;
        let id = incident["id"].as_str().expect("id");

        let mut client = sse::Client::connect(
            addr,
            &format!("/api/v1/incidents/{id}/stream"),
            &w.budi,
            None,
        )
        .await;
        assert_eq!(client.frame().await, "retry: 3000\n\n");

        let mut admin = w.app.state.redis.clone();
        let _: i64 = redis::cmd("CLIENT")
            .arg("KILL")
            .arg("TYPE")
            .arg("pubsub")
            .query_async(&mut admin)
            .await
            .expect("kill");
        tokio::time::sleep(Duration::from_millis(1500)).await;

        let (status, _) = w
            .reply(&w.andi, id, json!({"message": "setelah putus"}))
            .await;
        assert_eq!(status, StatusCode::CREATED);
        assert!(client.frame().await.contains("setelah putus"));
    }
}
