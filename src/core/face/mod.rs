//! Face recognition infrastructure: photo decoding, the inference engine, and at-rest encryption
//! of embeddings and photos.
//!
//! Fail closed: [`Faces::load`] yields an unavailable handle unless the ONNX Runtime library, both
//! models and the encryption key are configured and load cleanly. Callers must refuse every face
//! operation then; there is deliberately no "always match" engine outside the test helpers.

mod onnx;

use std::io::{self, Cursor};
use std::path::{Component, Path, PathBuf};
use std::sync::Arc;

use aws_lc_rs::aead::{AES_256_GCM, Aad, LessSafeKey, NONCE_LEN, Nonce, UnboundKey};
use aws_lc_rs::error::Unspecified;
use chrono::{Datelike, Utc};
use image::codecs::jpeg::JpegEncoder;
use image::imageops::{self, FilterType};
use image::metadata::Orientation;
use image::{DynamicImage, ImageDecoder, ImageFormat, ImageReader, Limits, RgbImage};
use tokio::fs;
use tokio::io::AsyncWriteExt;
use uuid::Uuid;

use crate::core::config::FaceConfig;

pub use image::Rgb;

/// Stored photos are re-encoded to this size: enough for an admin to recognise the person, small
/// on disk, and free of the original EXIF (GPS) metadata.
const STORED_MAX_SIDE: u32 = 640;
const STORED_JPEG_QUALITY: u8 = 85;
const MAX_DECODE_SIDE: u32 = 8192;
const MAX_DECODE_ALLOC: u64 = 256 * 1024 * 1024;

/// Why a photo cannot be used. Everything but [`Engine`](Self::Engine) is the photo's fault.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum FaceError {
    /// Inference failed at runtime; the cause is for the log only.
    Engine(String),
    InvalidImage,
    NoFace,
    MultipleFaces,
    TooSmall,
    OffCenter,
    Tilted,
    TooDark,
    TooBright,
    Unclear,
}

impl FaceError {
    /// Indonesian text for the user.
    pub fn message(&self) -> &'static str {
        match self {
            Self::Engine(_) => "Layanan pengenalan wajah sedang bermasalah. Coba lagi nanti.",
            Self::InvalidImage => "Foto tidak dapat dibaca. Kirim foto berformat JPEG.",
            Self::NoFace => "Wajah tidak terdeteksi.",
            Self::MultipleFaces => "Terdeteksi lebih dari satu wajah.",
            Self::TooSmall => "Wajah terlalu jauh dari kamera.",
            Self::OffCenter => "Wajah terpotong; posisikan wajah di tengah.",
            Self::Tilted => "Hadapkan wajah lurus ke kamera.",
            Self::TooDark => "Foto terlalu gelap.",
            Self::TooBright => "Foto terlalu terang.",
            Self::Unclear => "Wajah kurang jelas.",
        }
    }
}

/// Detector plus embedder. Implementations are called from blocking threads.
pub trait FaceEngine: Send + Sync {
    /// Identifies the embedder weights; embeddings of different versions are not comparable.
    fn model_version(&self) -> &str;

    /// The L2-normalised embedding of the single usable face in `image`, after the quality gate.
    ///
    /// # Errors
    /// The quality reason, or [`FaceError::Engine`].
    fn embed(&self, image: &RgbImage) -> Result<Vec<f32>, FaceError>;
}

/// Shared handle in `AppState`; `None` inside means face recognition is unavailable.
#[derive(Clone)]
pub struct Faces(Option<Arc<Ready>>);

/// A usable engine together with the encryption key.
pub struct Ready {
    engine: Arc<dyn FaceEngine>,
    key: LessSafeKey,
}

impl std::fmt::Debug for Faces {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match &self.0 {
            Some(ready) => write!(f, "Faces(ready, {})", ready.engine.model_version()),
            None => f.write_str("Faces(unavailable)"),
        }
    }
}

impl Faces {
    /// Loads the ONNX engine from the configuration, logging why when it stays unavailable.
    /// Blocking: reads and hashes the model files.
    pub fn load(config: &FaceConfig) -> Self {
        let (Some(library), Some(detector), Some(embedder)) = (
            &config.ort_library,
            &config.detector_model,
            &config.embedder_model,
        ) else {
            tracing::warn!(
                "[Face] FACE_ORT_LIBRARY, FACE_DETECTOR_MODEL or FACE_EMBEDDER_MODEL is unset; \
                 face registration and attendance are refused"
            );
            return Self(None);
        };
        if config.encryption_key.is_none() {
            tracing::warn!(
                "[Face] FACE_ENCRYPTION_KEY is missing or not 32 bytes of base64; face \
                 registration and attendance are refused"
            );
            return Self(None);
        }
        match onnx::OnnxEngine::load(library, detector, embedder) {
            Ok(engine) => {
                tracing::info!("[Face] models loaded ({})", engine.model_version());
                Self::with_engine(Arc::new(engine), config)
            }
            Err(err) => {
                tracing::error!("[Face] could not load the face models: {err}");
                Self(None)
            }
        }
    }

    /// A handle around any engine, still requiring the configured key. Tests use it to plug in
    /// a fake engine.
    pub fn with_engine(engine: Arc<dyn FaceEngine>, config: &FaceConfig) -> Self {
        let Some(key) = config.encryption_key else {
            return Self(None);
        };
        match UnboundKey::new(&AES_256_GCM, &key) {
            Ok(key) => Self(Some(Arc::new(Ready {
                engine,
                key: LessSafeKey::new(key),
            }))),
            Err(_) => Self(None),
        }
    }

    pub fn unavailable() -> Self {
        Self(None)
    }

    pub fn ready(&self) -> Option<&Arc<Ready>> {
        self.0.as_ref()
    }
}

impl Ready {
    pub fn model_version(&self) -> &str {
        self.engine.model_version()
    }

    /// Decodes and embeds on a blocking thread. Returns the downscaled JPEG to store (see
    /// [`stored_jpeg`]) instead of the decoded image, so full-size pixels are dropped early.
    ///
    /// # Errors
    /// See [`FaceEngine::embed`]; a panicking engine becomes [`FaceError::Engine`].
    pub async fn embed_photo(
        self: &Arc<Self>,
        jpeg: Vec<u8>,
    ) -> Result<(Vec<u8>, Vec<f32>), FaceError> {
        let this = Arc::clone(self);
        tokio::task::spawn_blocking(move || {
            let image = decode_photo(&jpeg)?;
            drop(jpeg);
            let embedding = this.engine.embed(&image)?;
            let stored = stored_jpeg(&image).map_err(|err| FaceError::Engine(err.to_string()))?;
            Ok((stored, embedding))
        })
        .await
        .map_err(|err| FaceError::Engine(err.to_string()))?
    }

    /// `nonce || ciphertext || tag`, bound to `aad` so a blob cannot be moved to another record.
    ///
    /// # Errors
    /// Only when the system random generator fails.
    pub fn seal(&self, plaintext: &[u8], aad: &str) -> Result<Vec<u8>, Unspecified> {
        let mut nonce = [0u8; NONCE_LEN];
        aws_lc_rs::rand::fill(&mut nonce)?;
        let mut buffer = plaintext.to_vec();
        self.key.seal_in_place_append_tag(
            Nonce::assume_unique_for_key(nonce),
            Aad::from(aad.as_bytes()),
            &mut buffer,
        )?;
        let mut out = nonce.to_vec();
        out.extend_from_slice(&buffer);
        Ok(out)
    }

    /// The plaintext, or `None` when the blob was tampered with, sealed for another `aad`, or
    /// sealed with another key.
    pub fn open(&self, sealed: &[u8], aad: &str) -> Option<Vec<u8>> {
        let (nonce, body) = sealed.split_at_checked(NONCE_LEN)?;
        let nonce = Nonce::try_assume_unique_for_key(nonce).ok()?;
        let mut buffer = body.to_vec();
        let plain = self
            .key
            .open_in_place(nonce, Aad::from(aad.as_bytes()), &mut buffer)
            .ok()?;
        Some(plain.to_vec())
    }

    /// Writes `jpeg` encrypted to `{dir}/{yyyy}/{mm}/{uuid}.bin` (atomic, owner-only) and returns
    /// the relative path, which is also the AAD.
    ///
    /// # Errors
    /// File system failures.
    pub async fn store_photo(&self, dir: &Path, jpeg: &[u8]) -> io::Result<String> {
        let now = Utc::now();
        let folder = format!("{:04}/{:02}", now.year(), now.month());
        let target_dir = dir.join(&folder);
        fs::DirBuilder::new()
            .recursive(true)
            .mode(0o700)
            .create(&target_dir)
            .await?;
        let id = Uuid::new_v4();
        let relative = format!("{folder}/{id}.bin");
        let sealed = self
            .seal(jpeg, &relative)
            .map_err(|_| io::Error::other("encryption failed"))?;
        let tmp = target_dir.join(format!(".{id}.tmp"));
        let result = async {
            let mut file = fs::OpenOptions::new()
                .write(true)
                .create_new(true)
                .mode(0o600)
                .open(&tmp)
                .await?;
            file.write_all(&sealed).await?;
            file.sync_all().await?;
            drop(file);
            fs::rename(&tmp, dir.join(&relative)).await
        }
        .await;
        if let Err(err) = result {
            let _ = fs::remove_file(&tmp).await;
            return Err(err);
        }
        Ok(relative)
    }

    /// The decrypted JPEG of a stored photo; `None` when missing, unsafe or not decryptable.
    pub async fn read_photo(&self, dir: &Path, relative: &str) -> Option<Vec<u8>> {
        let sealed = fs::read(resolve(dir, relative)?).await.ok()?;
        self.open(&sealed, relative)
    }
}

/// Removes a stored photo; already missing is fine.
pub async fn remove_photo(dir: &Path, relative: &str) {
    let Some(location) = resolve(dir, relative) else {
        tracing::warn!("[Face] refusing to remove unsafe photo path");
        return;
    };
    if let Err(err) = fs::remove_file(&location).await
        && err.kind() != io::ErrorKind::NotFound
    {
        tracing::warn!(error = %err, "[Face] could not remove a stored photo");
    }
}

/// Only plain `a/b/c` segments, so a stored path cannot leave `root`.
fn resolve(root: &Path, relative: &str) -> Option<PathBuf> {
    let path = Path::new(relative);
    (path.components().count() > 0 && path.components().all(|c| matches!(c, Component::Normal(_))))
        .then(|| root.join(path))
}

/// Decodes a JPEG, applying its EXIF orientation (phone cameras rotate through EXIF).
///
/// # Errors
/// [`FaceError::InvalidImage`] for anything that is not a decodable JPEG within the limits.
pub fn decode_photo(bytes: &[u8]) -> Result<RgbImage, FaceError> {
    let mut reader = ImageReader::with_format(Cursor::new(bytes), ImageFormat::Jpeg);
    let mut limits = Limits::default();
    limits.max_image_width = Some(MAX_DECODE_SIDE);
    limits.max_image_height = Some(MAX_DECODE_SIDE);
    limits.max_alloc = Some(MAX_DECODE_ALLOC);
    reader.limits(limits);
    let mut decoder = reader.into_decoder().map_err(|_| FaceError::InvalidImage)?;
    let orientation = decoder.orientation().unwrap_or(Orientation::NoTransforms);
    let mut image = DynamicImage::from_decoder(decoder).map_err(|_| FaceError::InvalidImage)?;
    image.apply_orientation(orientation);
    Ok(image.into_rgb8())
}

/// The image downscaled to [`STORED_MAX_SIDE`] and encoded as JPEG.
///
/// # Errors
/// Encoder failures (not expected for an in-memory RGB image).
pub fn stored_jpeg(image: &RgbImage) -> Result<Vec<u8>, image::ImageError> {
    let (width, height) = image.dimensions();
    let longest = width.max(height);
    let scaled;
    let image = if longest > STORED_MAX_SIDE {
        let ratio = f64::from(STORED_MAX_SIDE) / f64::from(longest);
        let scale = |side: u32| ((f64::from(side) * ratio).round() as u32).max(1);
        scaled = imageops::resize(image, scale(width), scale(height), FilterType::Triangle);
        &scaled
    } else {
        image
    };
    let mut out = Vec::new();
    JpegEncoder::new_with_quality(&mut out, STORED_JPEG_QUALITY).encode_image(image)?;
    Ok(out)
}

pub fn embedding_to_bytes(embedding: &[f32]) -> Vec<u8> {
    embedding.iter().flat_map(|v| v.to_le_bytes()).collect()
}

/// `None` when the length is not a whole number of `f32`.
pub fn embedding_from_bytes(bytes: &[u8]) -> Option<Vec<f32>> {
    if bytes.is_empty() || !bytes.len().is_multiple_of(4) {
        return None;
    }
    let (chunks, _) = bytes.as_chunks::<4>();
    Some(chunks.iter().map(|c| f32::from_le_bytes(*c)).collect())
}

/// Cosine similarity; 0 for mismatched lengths or zero vectors, so they never match.
pub fn cosine(a: &[f32], b: &[f32]) -> f32 {
    if a.len() != b.len() || a.is_empty() {
        return 0.0;
    }
    let (mut dot, mut na, mut nb) = (0.0f32, 0.0f32, 0.0f32);
    for (x, y) in a.iter().zip(b) {
        dot += x * y;
        na += x * x;
        nb += y * y;
    }
    if na == 0.0 || nb == 0.0 {
        return 0.0;
    }
    dot / (na.sqrt() * nb.sqrt())
}

/// Scales to unit length; a zero vector stays zero.
pub fn normalize(mut v: Vec<f32>) -> Vec<f32> {
    let norm = v.iter().map(|x| x * x).sum::<f32>().sqrt();
    if norm > 0.0 {
        v.iter_mut().for_each(|x| *x /= norm);
    }
    v
}

/// The re-normalised mean of normalised samples of equal length; `None` when empty or uneven.
pub fn mean_embedding(samples: &[Vec<f32>]) -> Option<Vec<f32>> {
    let dim = samples.first()?.len();
    if dim == 0 || samples.iter().any(|s| s.len() != dim) {
        return None;
    }
    let mut sum = vec![0.0f32; dim];
    for sample in samples {
        for (acc, x) in sum.iter_mut().zip(normalize(sample.clone())) {
            *acc += x;
        }
    }
    Some(normalize(sum))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ready() -> Arc<Ready> {
        struct Never;
        impl FaceEngine for Never {
            fn model_version(&self) -> &str {
                "never"
            }
            fn embed(&self, _: &RgbImage) -> Result<Vec<f32>, FaceError> {
                Err(FaceError::NoFace)
            }
        }
        let config = FaceConfig {
            encryption_key: Some([9; 32]),
            ..FaceConfig::default()
        };
        Faces::with_engine(Arc::new(Never), &config)
            .ready()
            .cloned()
            .expect("ready with a key")
    }

    #[test]
    fn no_key_means_unavailable() {
        struct Any;
        impl FaceEngine for Any {
            fn model_version(&self) -> &str {
                "x"
            }
            fn embed(&self, _: &RgbImage) -> Result<Vec<f32>, FaceError> {
                Ok(vec![1.0])
            }
        }
        assert!(
            Faces::with_engine(Arc::new(Any), &FaceConfig::default())
                .ready()
                .is_none()
        );
        assert!(Faces::load(&FaceConfig::default()).ready().is_none());
    }

    #[test]
    fn sealed_blobs_open_only_with_the_same_aad() {
        let ready = ready();
        let sealed = ready.seal(b"embedding", "a@x.id").expect("seal");
        assert_ne!(&sealed[NONCE_LEN..], b"embedding");
        assert_eq!(
            ready.open(&sealed, "a@x.id").as_deref(),
            Some(&b"embedding"[..])
        );
        assert_eq!(ready.open(&sealed, "b@x.id"), None);
        let mut tampered = sealed.clone();
        if let Some(last) = tampered.last_mut() {
            *last ^= 1;
        }
        assert_eq!(ready.open(&tampered, "a@x.id"), None);
        assert_eq!(ready.open(&sealed[..4], "a@x.id"), None);
    }

    #[test]
    fn cosine_and_mean() {
        assert!((cosine(&[1.0, 0.0], &[2.0, 0.0]) - 1.0).abs() < 1e-6);
        assert!(cosine(&[1.0, 0.0], &[0.0, 1.0]).abs() < 1e-6);
        assert!((cosine(&[1.0, 0.0], &[-1.0, 0.0]) + 1.0).abs() < 1e-6);
        assert_eq!(cosine(&[1.0], &[1.0, 0.0]), 0.0);
        assert_eq!(cosine(&[0.0, 0.0], &[1.0, 0.0]), 0.0);

        let mean = mean_embedding(&[vec![1.0, 0.0], vec![0.0, 3.0]]).expect("mean");
        let half = std::f32::consts::FRAC_1_SQRT_2;
        assert!((mean[0] - half).abs() < 1e-6 && (mean[1] - half).abs() < 1e-6);
        assert_eq!(mean_embedding(&[]), None);
        assert_eq!(mean_embedding(&[vec![1.0], vec![1.0, 2.0]]), None);
    }

    #[test]
    fn embedding_bytes_round_trip() {
        let v = vec![0.25f32, -1.5, 3.0];
        assert_eq!(embedding_from_bytes(&embedding_to_bytes(&v)), Some(v));
        assert_eq!(embedding_from_bytes(&[1, 2, 3]), None);
        assert_eq!(embedding_from_bytes(&[]), None);
    }

    #[test]
    fn photos_decode_and_shrink() {
        let image = RgbImage::from_pixel(1280, 960, Rgb([120, 90, 60]));
        let jpeg = stored_jpeg(&image).expect("encode");
        let decoded = decode_photo(&jpeg).expect("decode");
        assert_eq!(decoded.dimensions(), (640, 480));
        assert_eq!(decode_photo(b"not a jpeg"), Err(FaceError::InvalidImage));
    }

    #[test]
    fn stored_paths_cannot_leave_the_root() {
        let root = Path::new("/srv/faces");
        assert!(resolve(root, "2026/11/a.bin").is_some());
        for bad in ["../x", "/etc/passwd", "2026/../../x", "./a", ""] {
            assert_eq!(resolve(root, bad), None, "{bad}");
        }
    }
}
