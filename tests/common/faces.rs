//! Fake face engine for integration tests. It exists only in the test crate: the server itself has
//! no engine that matches without real models.
//!
//! A "face" is a solid-colour JPEG. The dominant channel is the identity (red, green and blue are
//! three different people with cosine 0 between them); near-black has no face and grey fails the
//! quality gate.

use std::path::Path;
use std::sync::Arc;

use image::codecs::jpeg::JpegEncoder;
use image::{Rgb, RgbImage};
use kawalo_api::core::config::Config;
use kawalo_api::core::face::{FaceEngine, FaceError, Faces};
use kawalo_api::core::state::AppState;
use tempfile::TempDir;

use super::TestApp;

pub const RED: [u8; 3] = [220, 30, 30];
pub const GREEN: [u8; 3] = [30, 200, 40];
pub const BLUE: [u8; 3] = [30, 40, 210];
pub const GREY: [u8; 3] = [128, 128, 128];
pub const BLACK: [u8; 3] = [0, 0, 0];

pub struct FakeFaceEngine;

impl FaceEngine for FakeFaceEngine {
    fn model_version(&self) -> &str {
        "fake-v1"
    }

    fn embed(&self, image: &RgbImage) -> Result<Vec<f32>, FaceError> {
        let n = image.pixels().len().max(1) as f32;
        let mut mean = [0.0f32; 3];
        for p in image.pixels() {
            for c in 0..3 {
                mean[c] += f32::from(p[c]) / n;
            }
        }
        let max = mean.iter().copied().fold(0.0, f32::max);
        let min = mean.iter().copied().fold(255.0, f32::min);
        if max < 16.0 {
            return Err(FaceError::NoFace);
        }
        if max - min < 20.0 {
            return Err(FaceError::TooSmall);
        }
        let mut embedding = vec![0.0f32; 8];
        let dominant = mean
            .iter()
            .enumerate()
            .max_by(|a, b| a.1.total_cmp(b.1))
            .map_or(0, |(i, _)| i);
        embedding[dominant] = 1.0;
        Ok(embedding)
    }
}

/// A 160x160 solid-colour JPEG.
pub fn face_jpeg(rgb: [u8; 3]) -> Vec<u8> {
    let image = RgbImage::from_pixel(160, 160, Rgb(rgb));
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, 95)
        .encode_image(&image)
        .expect("encode test face");
    out
}

/// Sets the encryption key and a private photo directory.
pub fn configure(config: &mut Config, photo_dir: &Path) {
    config.faces.encryption_key = Some([42; 32]);
    config.faces.photo_dir = photo_dir.to_owned();
    config.faces.register_min_photos = 2;
}

fn plug_fake(state: &mut AppState) {
    state.faces = Faces::with_engine(Arc::new(FakeFaceEngine), &state.config.faces);
}

/// An app with the fake engine; keep the directory alive for the test's duration.
pub async fn start() -> (TestApp, TempDir) {
    let photos = tempfile::tempdir().expect("photo dir");
    let dir = photos.path().to_owned();
    let app = TestApp::start_with_state(|config| configure(config, &dir), plug_fake).await;
    (app, photos)
}

/// An app without any engine, as in a deployment without models.
pub async fn start_unavailable() -> (TestApp, TempDir) {
    let photos = tempfile::tempdir().expect("photo dir");
    let dir = photos.path().to_owned();
    let app = TestApp::start_with_state(
        |config| configure(config, &dir),
        |state| state.faces = Faces::unavailable(),
    )
    .await;
    (app, photos)
}

/// Registers `email` (already a member of a group) with two photos of `rgb`.
pub async fn register(
    app: &TestApp,
    token: &str,
    rgb: [u8; 3],
) -> (axum::http::StatusCode, serde_json::Value) {
    app.call_multipart(
        "/api/v1/faces/register",
        Some(token),
        &[
            super::Part::File("photos", face_jpeg(rgb)),
            super::Part::File("photos", face_jpeg(rgb)),
        ],
    )
    .await
}

/// A token with an arbitrary role (for `SUPER_ADMIN`, which `token_for` cannot mint).
pub async fn token_with_role(app: &TestApp, email: &str, group: &str, role: &str) -> String {
    use kawalo_api::core::security::{JwtUserPayload, sign_access_token};
    let session_ver = app
        .state
        .sessions
        .get_or_init_version(email)
        .await
        .expect("session version");
    let payload = JwtUserPayload {
        sub: Some(email.to_owned()),
        email: Some(email.to_owned()),
        name: Some("Penguji".to_owned()),
        role: Some(role.to_owned()),
        group_uid: Some(group.to_owned()),
        session_ver,
        ..JwtUserPayload::default()
    };
    sign_access_token(&app.state.keys, app.state.config.jwt_expires_in, &payload)
        .expect("sign token")
        .token
}

/// A joined membership, which the duplicate check and the admin group check read.
pub async fn join(app: &TestApp, group: &str, email: &str, category: &str) {
    sqlx::query(
        "INSERT INTO tkusergroup (Email, GroupUID, Category, Status) VALUES (?, ?, ?, 'JOINED')",
    )
    .bind(email)
    .bind(group)
    .bind(category)
    .execute(&app.state.db)
    .await
    .expect("insert membership");
}

pub fn unique() -> String {
    uuid::Uuid::new_v4().simple().to_string()[..12].to_owned()
}
