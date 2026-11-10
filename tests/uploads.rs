//! Integration tests for `POST /api/v1/uploads` and `GET /uploads/...`; they need the scratch
//! MariaDB and Redis (see `scripts/dev-services.sh`).

mod common;

use std::net::SocketAddr;

use axum::body::Body;
use axum::extract::ConnectInfo;
use axum::http::{HeaderMap, Method, Request, StatusCode, header};
use common::TestApp;
use http_body_util::BodyExt;
use serde_json::Value;
use tempfile::TempDir;
use tower::ServiceExt;
use uuid::Uuid;

const BOUNDARY: &str = "kawaloBoundary7MA4YWxkTrZu0gW";

struct Env {
    app: TestApp,
    dir: TempDir,
    token: String,
}

impl Env {
    async fn new() -> Self {
        let dir = tempfile::tempdir().expect("upload dir");
        let root = dir.path().to_owned();
        let app = TestApp::start_with(move |config| config.upload_dir = root).await;
        let token = app
            .token_for(
                &format!("up-{}@x.id", Uuid::new_v4().simple()),
                "g-up",
                false,
            )
            .await;
        Self { app, dir, token }
    }

    async fn send(&self, request: Request<Body>) -> (StatusCode, HeaderMap, Vec<u8>) {
        let mut request = request;
        request.extensions_mut().insert(ConnectInfo(
            "127.0.0.1:50000".parse::<SocketAddr>().expect("peer"),
        ));
        let response = self
            .app
            .router
            .clone()
            .oneshot(request)
            .await
            .expect("infallible router");
        let (parts, body) = response.into_parts();
        let bytes = body.collect().await.expect("body").to_bytes().to_vec();
        (parts.status, parts.headers, bytes)
    }

    async fn post(&self, token: Option<&str>, body: Vec<u8>) -> (StatusCode, HeaderMap, Value) {
        let mut builder = Request::builder()
            .method(Method::POST)
            .uri("/api/v1/uploads")
            .header(header::HOST, "api.test:3000")
            .header(
                header::CONTENT_TYPE,
                format!("multipart/form-data; boundary={BOUNDARY}"),
            );
        if let Some(token) = token {
            builder = builder.header(header::AUTHORIZATION, format!("Bearer {token}"));
        }
        let (status, headers, bytes) = self
            .send(builder.body(Body::from(body)).expect("request"))
            .await;
        (
            status,
            headers,
            serde_json::from_slice(&bytes).unwrap_or(Value::Null),
        )
    }

    /// Uploads with the caller's token.
    async fn upload(&self, kind: Option<&str>, file: Option<(&str, &[u8])>) -> (StatusCode, Value) {
        let (status, _, json) = self.post(Some(&self.token), multipart(kind, file)).await;
        (status, json)
    }

    async fn get(&self, method: Method, path: &str) -> (StatusCode, HeaderMap, Vec<u8>) {
        self.send(
            Request::builder()
                .method(method)
                .uri(path)
                .body(Body::empty())
                .expect("request"),
        )
        .await
    }
}

fn multipart(kind: Option<&str>, file: Option<(&str, &[u8])>) -> Vec<u8> {
    let mut body = Vec::new();
    if let Some(kind) = kind {
        body.extend(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"kind\"\r\n\r\n{kind}\r\n"
            )
            .into_bytes(),
        );
    }
    if let Some((name, data)) = file {
        body.extend(
            format!(
                "--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"file\"; filename=\"{name}\"\r\nContent-Type: image/webp\r\n\r\n"
            )
            .into_bytes(),
        );
        body.extend(data);
        body.extend(b"\r\n");
    }
    body.extend(format!("--{BOUNDARY}--\r\n").into_bytes());
    body
}

/// A structurally valid lossless WebP of exactly `len` bytes.
fn webp(len: usize) -> Vec<u8> {
    assert!(len >= 26);
    let mut out = b"RIFF".to_vec();
    out.extend(u32::try_from(len - 8).expect("size").to_le_bytes());
    out.extend(b"WEBPVP8L");
    out.extend(u32::try_from(len - 20).expect("size").to_le_bytes());
    out.push(0x2F);
    out.resize(len, 0x5A);
    out
}

const PNG: &[u8] =
    b"\x89PNG\r\n\x1a\n\0\0\0\rIHDR\0\0\0\x01\0\0\0\x01\x08\x06\0\0\0\x1f\x15\xc4\x89";
const JPEG: &[u8] = b"\xFF\xD8\xFF\xE0\0\x10JFIF\0\x01\x01\0\0\x01\0\x01\0\0\xFF\xD9";

fn assert_error(status: StatusCode, json: &Value, want: u16, code: &str, message: &str) {
    assert_eq!(status.as_u16(), want, "{json}");
    assert_eq!(json["success"], false);
    assert_eq!(json["error_code"], code, "{json}");
    assert_eq!(json["message"], message, "{json}");
}

fn path_of(url: &str) -> &str {
    url.strip_prefix("http://api.test:3000").expect("url base")
}

#[tokio::test]
async fn image_upload_is_stored_and_served_back_identically() {
    let env = Env::new().await;
    let data = webp(2000);
    let (status, json) = env
        .upload(Some("image"), Some(("../../foto kamera.webp", &data)))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    assert_eq!(json["success"], true);
    assert_eq!(json["message"], "Berkas berhasil diunggah");
    let body = &json["data"];
    assert_eq!(body["kind"], "image");
    assert_eq!(body["size"], 2000);
    assert_eq!(body["content_type"], "image/webp");
    assert_eq!(body["name"], "foto kamera.webp");
    let url = body["url"].as_str().expect("url");
    assert!(url.starts_with("http://api.test:3000/uploads/"), "{url}");
    assert!(url.ends_with(".webp") && url.len() < 2048);

    let relative = path_of(url).strip_prefix("/uploads/").expect("prefix");
    let on_disk = env.dir.path().join(relative);
    assert_eq!(std::fs::read(&on_disk).expect("stored file"), data);
    assert!(
        std::fs::read_dir(on_disk.parent().expect("dir"))
            .expect("list")
            .all(|e| !e
                .expect("entry")
                .file_name()
                .to_string_lossy()
                .ends_with(".tmp"))
    );
    use std::os::unix::fs::PermissionsExt;
    assert_eq!(
        std::fs::metadata(&on_disk)
            .expect("meta")
            .permissions()
            .mode()
            & 0o777,
        0o644
    );

    let (status, headers, bytes) = env.get(Method::GET, path_of(url)).await;
    assert_eq!(status, StatusCode::OK);
    assert_eq!(bytes, data);
    assert_eq!(headers[header::CONTENT_TYPE], "image/webp");
    assert_eq!(headers[header::CONTENT_LENGTH], "2000");
    assert_eq!(
        headers[header::CACHE_CONTROL],
        "public, max-age=31536000, immutable"
    );
    assert_eq!(headers[header::CONTENT_DISPOSITION], "inline");
    assert_eq!(headers["x-content-type-options"], "nosniff");
    assert_eq!(headers["x-frame-options"], "DENY");

    let (status, headers, bytes) = env.get(Method::HEAD, path_of(url)).await;
    assert_eq!(status, StatusCode::OK);
    assert!(bytes.is_empty());
    assert_eq!(headers[header::CONTENT_LENGTH], "2000");
}

#[tokio::test]
async fn public_base_url_overrides_the_host_header() {
    let dir = tempfile::tempdir().expect("dir");
    let root = dir.path().to_owned();
    let app = TestApp::start_with(move |config| {
        config.upload_dir = root;
        config.public_base_url = "https://api.contoh.id".to_owned();
    })
    .await;
    let token = app.token_for("pb@x.id", "g-up", false).await;
    let env = Env { app, dir, token };
    let (status, json) = env
        .upload(Some("image"), Some(("a.webp", &webp(100))))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    assert!(
        json["data"]["url"]
            .as_str()
            .expect("url")
            .starts_with("https://api.contoh.id/uploads/")
    );
}

#[tokio::test]
async fn disguised_and_broken_images_are_rejected() {
    let env = Env::new().await;
    let mut trailing = webp(100);
    trailing.push(0);
    let mut truncated = webp(100);
    truncated.truncate(60);
    let cases: [(&str, &[u8]); 5] = [
        ("png", PNG),
        ("jpeg", JPEG),
        ("trailing", &trailing),
        ("truncated", &truncated),
        ("text", b"RIFF ini bukan gambar"),
    ];
    for (_label, data) in cases {
        let (status, json) = env.upload(Some("image"), Some(("x.webp", data))).await;
        assert_error(
            status,
            &json,
            422,
            "VALIDATION_ERROR",
            "Gambar harus berformat WebP yang valid.",
        );
    }
    let (status, json) = env.upload(Some("image"), Some(("x.webp", b""))).await;
    assert_error(
        status,
        &json,
        422,
        "VALIDATION_ERROR",
        "Berkas tidak boleh kosong.",
    );
}

#[tokio::test]
async fn image_size_limit_is_150000_bytes_inclusive() {
    let env = Env::new().await;
    let (status, json) = env
        .upload(Some("image"), Some(("x.webp", &webp(150_000))))
        .await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    assert_eq!(json["data"]["size"], 150_000);
    for len in [150_001, 3_000_000] {
        let (status, json) = env
            .upload(Some("image"), Some(("x.webp", &webp(len))))
            .await;
        assert_error(
            status,
            &json,
            413,
            "BAD_REQUEST",
            "Ukuran gambar maksimal 150 KB.",
        );
    }
}

#[tokio::test]
async fn image_over_the_limit_is_rejected_even_when_file_precedes_kind() {
    let env = Env::new().await;
    let mut body = multipart(None, Some(("x.webp", &webp(150_001))));
    body.truncate(body.len() - format!("--{BOUNDARY}--\r\n").len());
    body.extend(
        format!("--{BOUNDARY}\r\nContent-Disposition: form-data; name=\"kind\"\r\n\r\nimage\r\n--{BOUNDARY}--\r\n")
            .into_bytes(),
    );
    let (status, _, json) = env.post(Some(&env.token), body).await;
    assert_error(
        status,
        &json,
        413,
        "BAD_REQUEST",
        "Ukuran gambar maksimal 150 KB.",
    );
}

#[tokio::test]
async fn missing_or_wrong_fields_are_422() {
    let env = Env::new().await;
    let data = webp(100);
    let (status, json) = env.upload(Some("image"), None).await;
    assert_error(
        status,
        &json,
        422,
        "VALIDATION_ERROR",
        "Field file wajib diisi.",
    );
    let (status, json) = env.upload(None, Some(("x.webp", &data))).await;
    assert_error(
        status,
        &json,
        422,
        "VALIDATION_ERROR",
        "Field kind wajib diisi dengan image atau document.",
    );
    for kind in ["video", "", "IMAGE"] {
        let (status, json) = env.upload(Some(kind), Some(("x.webp", &data))).await;
        assert_error(
            status,
            &json,
            422,
            "VALIDATION_ERROR",
            "Field kind wajib diisi dengan image atau document.",
        );
    }
    let (status, _, json) = env
        .send(
            Request::builder()
                .method(Method::POST)
                .uri("/api/v1/uploads")
                .header(header::AUTHORIZATION, format!("Bearer {}", env.token))
                .header(header::CONTENT_TYPE, "application/json")
                .body(Body::from("{}"))
                .expect("request"),
        )
        .await
        .into_json();
    assert_error(
        status,
        &json,
        422,
        "VALIDATION_ERROR",
        "Permintaan harus berformat multipart/form-data.",
    );
}

trait IntoJson {
    fn into_json(self) -> (StatusCode, HeaderMap, Value);
}

impl IntoJson for (StatusCode, HeaderMap, Vec<u8>) {
    fn into_json(self) -> (StatusCode, HeaderMap, Value) {
        (
            self.0,
            self.1,
            serde_json::from_slice(&self.2).unwrap_or(Value::Null),
        )
    }
}

#[tokio::test]
async fn authentication_is_required() {
    let env = Env::new().await;
    let body = multipart(Some("image"), Some(("x.webp", &webp(100))));
    let (status, _, json) = env.post(None, body.clone()).await;
    assert_error(
        status,
        &json,
        401,
        "AUTH_UNAUTHORIZED",
        "Akses tidak diizinkan. Token tidak valid atau telah kedaluwarsa.",
    );
    let (status, _, json) = env.post(Some("bukan.token.jwt"), body.clone()).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED, "{json}");

    let email = format!("stale-{}@x.id", Uuid::new_v4().simple());
    let stale = env.app.token_for(&email, "g-up", false).await;
    env.app
        .state
        .sessions
        .increment_version(&email)
        .await
        .expect("bump");
    let (status, _, json) = env.post(Some(&stale), body).await;
    assert_eq!(status, StatusCode::UNAUTHORIZED);
    assert_eq!(json["error_code"], "AUTH_SESSION_STALE");
}

#[tokio::test]
async fn documents_are_validated_by_content_and_served_as_attachments() {
    let env = Env::new().await;
    let cases: [(&str, &[u8], &str); 6] = [
        ("laporan.pdf", b"%PDF-1.7\nisi", "application/pdf"),
        (
            "Surat.DOCX",
            b"PK\x03\x04rest",
            "application/vnd.openxmlformats-officedocument.wordprocessingml.document",
        ),
        (
            "data.xlsx",
            b"PK\x03\x04rest",
            "application/vnd.openxmlformats-officedocument.spreadsheetml.sheet",
        ),
        (
            "lama.doc",
            b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1rest",
            "application/msword",
        ),
        (
            "lama.xls",
            b"\xD0\xCF\x11\xE0\xA1\xB1\x1A\xE1rest",
            "application/vnd.ms-excel",
        ),
        (
            "catatan.txt",
            "halo \u{2713}\n".as_bytes(),
            "text/plain; charset=utf-8",
        ),
    ];
    for (name, data, content_type) in cases {
        let (status, json) = env.upload(Some("document"), Some((name, data))).await;
        assert_eq!(status, StatusCode::CREATED, "{name}: {json}");
        assert_eq!(json["data"]["kind"], "document");
        assert_eq!(json["data"]["content_type"], content_type);
        assert_eq!(json["data"]["name"], name);
        let url = json["data"]["url"].as_str().expect("url");
        let ext = name.rsplit('.').next().expect("ext").to_lowercase();
        assert!(url.ends_with(&format!(".{ext}")), "{url}");
        let (status, headers, bytes) = env.get(Method::GET, path_of(url)).await;
        assert_eq!(status, StatusCode::OK);
        assert_eq!(bytes, data);
        assert_eq!(headers[header::CONTENT_TYPE], content_type);
        assert_eq!(headers[header::CONTENT_DISPOSITION], "attachment");
    }

    let mismatch = "Isi berkas tidak sesuai dengan ekstensinya.";
    let bad: [(&str, &[u8]); 6] = [
        ("a.pdf", b"PK\x03\x04"),
        ("a.docx", b"%PDF-1.7"),
        ("a.xlsx", b"\x89PNG\r\n"),
        ("a.doc", b"PK\x03\x04"),
        ("a.txt", &[0xFF, 0xFE, 0x41]),
        ("a.txt", b"ab\0cd"),
    ];
    for (name, data) in bad {
        let (status, json) = env.upload(Some("document"), Some((name, data))).await;
        assert_error(status, &json, 422, "VALIDATION_ERROR", mismatch);
    }
    let unsupported = "Tipe dokumen tidak didukung. Gunakan pdf, doc, docx, xls, xlsx, atau txt.";
    for name in ["a.exe", "a.webp", "tanpa-ekstensi", ".pdf", "a.pdf.exe"] {
        let (status, json) = env
            .upload(Some("document"), Some((name, b"%PDF-1.7")))
            .await;
        assert_error(status, &json, 422, "VALIDATION_ERROR", unsupported);
    }
}

#[tokio::test]
async fn document_size_limit_is_10_mb() {
    let env = Env::new().await;
    let mut ok = b"%PDF-".to_vec();
    ok.resize(10 * 1024 * 1024, b'x');
    let (status, json) = env.upload(Some("document"), Some(("a.pdf", &ok))).await;
    assert_eq!(status, StatusCode::CREATED, "{json}");
    ok.push(b'x');
    let (status, json) = env.upload(Some("document"), Some(("a.pdf", &ok))).await;
    assert_error(
        status,
        &json,
        413,
        "BAD_REQUEST",
        "Ukuran dokumen maksimal 10 MB.",
    );
}

#[tokio::test]
async fn oversized_body_is_cut_off_by_the_route_limit() {
    let env = Env::new().await;
    let big = vec![b'x'; 11 * 1024 * 1024];
    let (status, json) = env.upload(Some("document"), Some(("a.txt", &big))).await;
    assert_error(
        status,
        &json,
        413,
        "BAD_REQUEST",
        "Ukuran dokumen maksimal 10 MB.",
    );
}

#[tokio::test]
async fn unknown_and_malicious_paths_are_404() {
    let env = Env::new().await;
    let (status, json) = env
        .upload(Some("image"), Some(("a.webp", &webp(100))))
        .await;
    assert_eq!(status, StatusCode::CREATED);
    let url = json["data"]["url"].as_str().expect("url");
    let good = path_of(url).to_owned();
    let name = good.rsplit('/').next().expect("name").to_owned();
    let id = Uuid::new_v4();

    let paths = [
        "/uploads/../../etc/passwd".to_owned(),
        "/uploads/2026/11/..%2F..%2Fsecret".to_owned(),
        format!("/uploads/%2e%2e/11/{name}"),
        "/uploads/2026/11/..".to_owned(),
        "/uploads/2026/11/.hidden".to_owned(),
        format!("/uploads/2026/11/{id}.webp"),
        format!("/uploads/2026/13/{id}.webp"),
        format!("/uploads/2026/11/{id}.exe"),
        format!("/uploads/2026/11/{}.webp", id.to_string().to_uppercase()),
        format!("/uploads/2026/11/{id}"),
        "/uploads".to_owned(),
        "/uploads/".to_owned(),
        good.replace(&name, &format!("{name}/x")),
    ];
    for path in paths {
        let (status, headers, bytes) = env.get(Method::GET, &path).await;
        assert_eq!(status, StatusCode::NOT_FOUND, "{path}");
        let json: Value = serde_json::from_slice(&bytes).expect("envelope");
        assert_eq!(json["success"], false, "{path}");
        assert_eq!(json["error_code"], "RESOURCE_NOT_FOUND", "{path}");
        assert_eq!(headers["x-content-type-options"], "nosniff");
    }
    assert!(!env.dir.path().join("etc").exists());
}

#[tokio::test]
async fn per_user_limit_returns_429_with_retry_after() {
    let env = Env::new().await;
    let body = multipart(Some("image"), Some(("a.webp", &webp(100))));
    for attempt in 1..=30 {
        let (status, _, json) = env.post(Some(&env.token), body.clone()).await;
        assert_eq!(status, StatusCode::CREATED, "attempt {attempt}: {json}");
    }
    let (status, headers, json) = env.post(Some(&env.token), body.clone()).await;
    assert_eq!(status, StatusCode::TOO_MANY_REQUESTS);
    assert_eq!(json["error_code"], "RATE_LIMIT_EXCEEDED");
    assert!(
        json["message"]
            .as_str()
            .expect("message")
            .starts_with("Terlalu banyak unggahan.")
    );
    let retry: u64 = headers[header::RETRY_AFTER]
        .to_str()
        .expect("header")
        .parse()
        .expect("seconds");
    assert!((1..=60).contains(&retry), "{retry}");

    let other = env
        .app
        .token_for(
            &format!("other-{}@x.id", Uuid::new_v4().simple()),
            "g-up",
            false,
        )
        .await;
    let (status, _, _) = env.post(Some(&other), body).await;
    assert_eq!(status, StatusCode::CREATED);
}
