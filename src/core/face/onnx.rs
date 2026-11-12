//! YuNet (detection with 5 keypoints) + an ArcFace-style 112x112 embedder on ONNX Runtime.
//!
//! Expected weights (loaded from disk, never committed): OpenCV Zoo
//! `face_detection_yunet_2023mar.onnx` (MIT) and AuraFace-v1 `glintr100.onnx` (Apache-2.0).
//!
//! The detector side follows OpenCV's `FaceDetectorYN` (`face_detect.cpp`): BGR input in 0..255
//! without normalisation, anchor-free decoding over strides 8/16/32 with
//! `score = sqrt(cls * obj)`, NMS at IoU 0.3. The image is letterboxed into the top-left corner so
//! one fixed-size model handles any photo. The embedder side follows InsightFace
//! (`face_align.py`, `arcface_onnx.py`): a least-squares similarity transform onto the ArcFace
//! landmark template, input normalised `(x - 127.5) / 127.5`, RGB, NCHW. Both were checked
//! against the Python references on LFW (boxes and landmarks identical to OpenCV; RGB scores
//! same-person pairs higher than BGR).

use std::path::Path;
use std::sync::Mutex;

use image::RgbImage;
use image::imageops::{self, FilterType};
use ort::environment::Environment;
use ort::session::Session;
use ort::value::Tensor;
use sha2::{Digest, Sha256};

use super::{FaceEngine, FaceError, normalize};

/// Used when the detector has a dynamic input size.
const DEFAULT_DET_SIZE: u32 = 640;
const DET_THRESHOLD: f32 = 0.5;
const NMS_IOU: f32 = 0.3;
const STRIDES: [u32; 3] = [8, 16, 32];
/// Per-stride output families of the YuNet export, named `{family}_{stride}`.
const OUTPUT_FAMILIES: [&str; 4] = ["cls", "obj", "bbox", "kps"];
const ARC_SIZE: u32 = 112;
/// ArcFace reference landmarks for a 112x112 crop: eyes, nose tip, mouth corners.
const ARC_TEMPLATE: [[f32; 2]; 5] = [
    [38.2946, 51.6963],
    [73.5318, 51.5014],
    [56.0252, 71.7366],
    [41.5493, 92.3655],
    [70.7299, 92.2041],
];
/// One intra-op thread: the server has 2 vCPUs shared with MySQL and other sites.
const INTRA_THREADS: usize = 1;

// Quality gate. ponytail: fixed values; move to configuration once real device photos show
// where they should sit.
const MIN_SCORE: f32 = 0.6;
const MIN_FACE_PX: f32 = 80.0;
/// A second face at least this fraction of the main face's area makes the photo ambiguous.
const SECOND_FACE_AREA: f32 = 0.25;
const MAX_ROLL_DEG: f32 = 20.0;
/// Nose offset from the eye midpoint, relative to the eye distance.
const MAX_YAW_RATIO: f32 = 0.35;
const EDGE_MARGIN: f32 = 0.05;
const MIN_LUMA: f32 = 50.0;
const MAX_LUMA: f32 = 210.0;

#[derive(Debug, Clone, PartialEq)]
pub(super) struct Detection {
    score: f32,
    /// x1, y1, x2, y2 in source image pixels.
    bbox: [f32; 4],
    /// Eyes, nose, mouth corners; within each pair the image-left point first, which is
    /// YuNet's order (subject's right eye first) and the ArcFace template's order alike.
    kps: [[f32; 2]; 5],
}

impl Detection {
    fn width(&self) -> f32 {
        self.bbox[2] - self.bbox[0]
    }

    fn height(&self) -> f32 {
        self.bbox[3] - self.bbox[1]
    }

    fn area(&self) -> f32 {
        self.width().max(0.0) * self.height().max(0.0)
    }
}

pub(super) struct OnnxEngine {
    detector: Mutex<Session>,
    embedder: Mutex<Session>,
    det_width: u32,
    det_height: u32,
    version: String,
    _environment: Environment,
}

fn engine_error(err: impl std::fmt::Display) -> FaceError {
    FaceError::Engine(err.to_string())
}

impl OnnxEngine {
    /// Loads the runtime library and both models, checking that they look like YuNet and an
    /// ArcFace-style embedder.
    pub(super) fn load(library: &Path, detector: &Path, embedder: &Path) -> Result<Self, String> {
        let environment = ort::init_from(library)
            .map_err(|err| format!("ONNX Runtime library {}: {err}", library.display()))?
            .with_name("kawalo-face")
            .build()
            .map_err(|err| format!("ONNX Runtime environment: {err}"))?;
        let session = |bytes: &[u8]| -> Result<Session, String> {
            Session::builder(&environment)
                .map_err(|err| err.to_string())?
                .with_intra_threads(INTRA_THREADS)
                .map_err(|err| err.to_string())?
                .commit_from_memory(bytes)
                .map_err(|err| err.to_string())
        };
        let detector_bytes = std::fs::read(detector)
            .map_err(|err| format!("detector model {}: {err}", detector.display()))?;
        let embedder_bytes = std::fs::read(embedder)
            .map_err(|err| format!("embedder model {}: {err}", embedder.display()))?;
        let detector_session =
            session(&detector_bytes).map_err(|err| format!("detector: {err}"))?;
        let embedder_session =
            session(&embedder_bytes).map_err(|err| format!("embedder: {err}"))?;

        let missing: Vec<String> = output_names()
            .filter(|name| !detector_session.outputs().iter().any(|o| o.name() == name))
            .collect();
        if !missing.is_empty() {
            return Err(format!(
                "detector must be YuNet (2023mar) with keypoints; missing outputs {}",
                missing.join(", ")
            ));
        }
        if embedder_session.inputs().len() != 1 || embedder_session.outputs().is_empty() {
            return Err("embedder must have one input and an embedding output".to_owned());
        }
        let (det_width, det_height) = detector_session
            .inputs()
            .first()
            .and_then(|input| input.dtype().tensor_shape())
            .and_then(|shape| match **shape {
                [_, _, h, w] if h > 0 && w > 0 => {
                    Some((u32::try_from(w).ok()?, u32::try_from(h).ok()?))
                }
                _ => None,
            })
            .unwrap_or((DEFAULT_DET_SIZE, DEFAULT_DET_SIZE));

        Ok(Self {
            detector: Mutex::new(detector_session),
            embedder: Mutex::new(embedder_session),
            det_width,
            det_height,
            version: model_version(&detector_bytes, &embedder_bytes),
            _environment: environment,
        })
    }

    fn detect(&self, image: &RgbImage) -> Result<Vec<Detection>, FaceError> {
        let (input, scale) = letterbox(image, self.det_width, self.det_height);
        let tensor = Tensor::from_array((
            [1usize, 3, self.det_height as usize, self.det_width as usize],
            input,
        ))
        .map_err(engine_error)?;
        let mut session = self
            .detector
            .lock()
            .map_err(|_| engine_error("detector lock poisoned"))?;
        let outputs = session.run(ort::inputs![tensor]).map_err(engine_error)?;
        let tensor = |family: &str, stride: u32| -> Result<&[f32], FaceError> {
            let (_, values) = outputs
                .get(format!("{family}_{stride}"))
                .ok_or_else(|| engine_error("detector output missing"))?
                .try_extract_tensor::<f32>()
                .map_err(engine_error)?;
            Ok(values)
        };
        let mut found = Vec::new();
        for stride in STRIDES {
            decode_level(
                Level {
                    cls: tensor("cls", stride)?,
                    obj: tensor("obj", stride)?,
                    boxes: tensor("bbox", stride)?,
                    kps: tensor("kps", stride)?,
                    stride,
                    width: self.det_width,
                    height: self.det_height,
                },
                scale,
                &mut found,
            )?;
        }
        Ok(nms(found))
    }

    fn arcface(&self, crop: &RgbImage) -> Result<Vec<f32>, FaceError> {
        let plane = (ARC_SIZE * ARC_SIZE) as usize;
        let mut input = vec![0.0f32; plane * 3];
        for (x, y, pixel) in crop.enumerate_pixels() {
            let i = (y * ARC_SIZE + x) as usize;
            for c in 0..3 {
                input[c * plane + i] = (f32::from(pixel[c]) - 127.5) / 127.5;
            }
        }
        let tensor = Tensor::from_array(([1usize, 3, ARC_SIZE as usize, ARC_SIZE as usize], input))
            .map_err(engine_error)?;
        let mut session = self
            .embedder
            .lock()
            .map_err(|_| engine_error("embedder lock poisoned"))?;
        let outputs = session.run(ort::inputs![tensor]).map_err(engine_error)?;
        let (_, values) = outputs[0]
            .try_extract_tensor::<f32>()
            .map_err(engine_error)?;
        if values.is_empty() || values.iter().any(|v| !v.is_finite()) {
            return Err(engine_error("embedder produced an invalid vector"));
        }
        Ok(normalize(values.to_vec()))
    }
}

impl FaceEngine for OnnxEngine {
    fn model_version(&self) -> &str {
        &self.version
    }

    fn embed(&self, image: &RgbImage) -> Result<Vec<f32>, FaceError> {
        let detections = self.detect(image)?;
        let face = assess(&detections, image.width() as f32, image.height() as f32)?;
        let luma = mean_luma(image, &face.bbox);
        if luma < MIN_LUMA {
            return Err(FaceError::TooDark);
        }
        if luma > MAX_LUMA {
            return Err(FaceError::TooBright);
        }
        let crop = align(image, &face.kps);
        self.arcface(&crop)
    }
}

fn output_names() -> impl Iterator<Item = String> {
    OUTPUT_FAMILIES
        .iter()
        .flat_map(|family| STRIDES.iter().map(move |s| format!("{family}_{s}")))
}

/// Stored with every embedding. Both weights are hashed because the detector's landmarks decide
/// the aligned crop, so swapping either one makes old embeddings incomparable.
fn model_version(detector: &[u8], embedder: &[u8]) -> String {
    let det = hex::encode(Sha256::digest(detector));
    let emb = hex::encode(Sha256::digest(embedder));
    format!("yunet-{}-arcface-{}", &det[..8], &emb[..16])
}

/// Detector input (NCHW, BGR, raw 0..255) with the image resized into the top-left corner keeping
/// its aspect ratio and black padding, and the factor from source to detector pixels.
fn letterbox(image: &RgbImage, width: u32, height: u32) -> (Vec<f32>, f32) {
    let (src_w, src_h) = image.dimensions();
    let image_ratio = src_h as f32 / src_w as f32;
    let model_ratio = height as f32 / width as f32;
    let (new_w, new_h) = if image_ratio > model_ratio {
        ((height as f32 / image_ratio) as u32, height)
    } else {
        (width, (width as f32 * image_ratio) as u32)
    };
    let (new_w, new_h) = (new_w.clamp(1, width), new_h.clamp(1, height));
    let scale = new_h as f32 / src_h as f32;
    let resized = imageops::resize(image, new_w, new_h, FilterType::Triangle);
    let plane = (width * height) as usize;
    let mut input = vec![0.0f32; plane * 3];
    for (x, y, pixel) in resized.enumerate_pixels() {
        let i = (y * width + x) as usize;
        for c in 0..3 {
            input[(2 - c) * plane + i] = f32::from(pixel[c]);
        }
    }
    (input, scale)
}

struct Level<'a> {
    cls: &'a [f32],
    obj: &'a [f32],
    boxes: &'a [f32],
    kps: &'a [f32],
    stride: u32,
    width: u32,
    height: u32,
}

/// One prediction per cell (row-major). Box centre and landmarks are offsets from the cell's
/// top-left corner and the box size is log-scale, all in stride units. Results are mapped back
/// to source pixels with `scale`.
fn decode_level(level: Level<'_>, scale: f32, out: &mut Vec<Detection>) -> Result<(), FaceError> {
    let cols = level.width / level.stride;
    let rows = level.height / level.stride;
    let count = (cols * rows) as usize;
    if level.cls.len() != count
        || level.obj.len() != count
        || level.boxes.len() != count * 4
        || level.kps.len() != count * 10
    {
        return Err(engine_error("unexpected detector output shape"));
    }
    let stride = level.stride as f32;
    for (k, (&cls, &obj)) in level.cls.iter().zip(level.obj).enumerate() {
        let score = (cls.clamp(0.0, 1.0) * obj.clamp(0.0, 1.0)).sqrt();
        if score < DET_THRESHOLD {
            continue;
        }
        let col = (k as u32 % cols) as f32;
        let row = (k as u32 / cols) as f32;
        let d = &level.boxes[k * 4..k * 4 + 4];
        let (cx, cy) = ((col + d[0]) * stride, (row + d[1]) * stride);
        let (w, h) = (d[2].exp() * stride, d[3].exp() * stride);
        let bbox = [
            (cx - w / 2.0) / scale,
            (cy - h / 2.0) / scale,
            (cx + w / 2.0) / scale,
            (cy + h / 2.0) / scale,
        ];
        let p = &level.kps[k * 10..k * 10 + 10];
        let mut kps = [[0.0f32; 2]; 5];
        for (i, point) in kps.iter_mut().enumerate() {
            *point = [
                (col + p[i * 2]) * stride / scale,
                (row + p[i * 2 + 1]) * stride / scale,
            ];
        }
        out.push(Detection { score, bbox, kps });
    }
    Ok(())
}

fn iou(a: &Detection, b: &Detection) -> f32 {
    let w = (a.bbox[2].min(b.bbox[2]) - a.bbox[0].max(b.bbox[0])).max(0.0);
    let h = (a.bbox[3].min(b.bbox[3]) - a.bbox[1].max(b.bbox[1])).max(0.0);
    let inter = w * h;
    let union = a.area() + b.area() - inter;
    if union <= 0.0 { 0.0 } else { inter / union }
}

/// Greedy non-maximum suppression, highest score first.
fn nms(mut detections: Vec<Detection>) -> Vec<Detection> {
    detections.sort_by(|a, b| b.score.total_cmp(&a.score));
    let mut kept: Vec<Detection> = Vec::new();
    for detection in detections {
        if kept.iter().all(|k| iou(k, &detection) <= NMS_IOU) {
            kept.push(detection);
        }
    }
    kept
}

/// The single usable face, or why the photo is not good enough.
fn assess(detections: &[Detection], width: f32, height: f32) -> Result<&Detection, FaceError> {
    let main = detections
        .iter()
        .max_by(|a, b| a.area().total_cmp(&b.area()))
        .ok_or(FaceError::NoFace)?;
    let others = detections
        .iter()
        .filter(|d| !std::ptr::eq(*d, main) && d.area() >= main.area() * SECOND_FACE_AREA)
        .count();
    if others > 0 {
        return Err(FaceError::MultipleFaces);
    }
    if main.score < MIN_SCORE {
        return Err(FaceError::Unclear);
    }
    let (w, h) = (main.width(), main.height());
    if w.min(h) < MIN_FACE_PX {
        return Err(FaceError::TooSmall);
    }
    let [x1, y1, x2, y2] = main.bbox;
    if x1 < -EDGE_MARGIN * w
        || y1 < -EDGE_MARGIN * h
        || x2 > width + EDGE_MARGIN * w
        || y2 > height + EDGE_MARGIN * h
    {
        return Err(FaceError::OffCenter);
    }
    let [left, right, nose, ..] = main.kps;
    let (dx, dy) = (right[0] - left[0], right[1] - left[1]);
    let eye_distance = dx.hypot(dy);
    if eye_distance <= 0.0 || dy.atan2(dx).to_degrees().abs() > MAX_ROLL_DEG {
        return Err(FaceError::Tilted);
    }
    let eye_mid = (left[0] + right[0]) / 2.0;
    if (nose[0] - eye_mid).abs() / eye_distance > MAX_YAW_RATIO {
        return Err(FaceError::Tilted);
    }
    Ok(main)
}

/// Mean BT.601 luma inside the box, sampled on a coarse grid.
fn mean_luma(image: &RgbImage, bbox: &[f32; 4]) -> f32 {
    let clamp_x = |v: f32| (v.max(0.0) as u32).min(image.width().saturating_sub(1));
    let clamp_y = |v: f32| (v.max(0.0) as u32).min(image.height().saturating_sub(1));
    let (x1, y1, x2, y2) = (
        clamp_x(bbox[0]),
        clamp_y(bbox[1]),
        clamp_x(bbox[2]),
        clamp_y(bbox[3]),
    );
    let step = ((x2 - x1).max(y2 - y1) / 32).max(1) as usize;
    let (mut sum, mut count) = (0.0f32, 0.0f32);
    for y in (y1..=y2).step_by(step) {
        for x in (x1..=x2).step_by(step) {
            let p = image.get_pixel(x, y);
            sum += 0.299 * f32::from(p[0]) + 0.587 * f32::from(p[1]) + 0.114 * f32::from(p[2]);
            count += 1.0;
        }
    }
    if count == 0.0 { 0.0 } else { sum / count }
}

/// Least-squares similarity `[a, b, tx, ty]` mapping `src` onto `dst`:
/// `x' = a*x - b*y + tx`, `y' = b*x + a*y + ty`.
fn similarity(src: &[[f32; 2]; 5], dst: &[[f32; 2]; 5]) -> [f32; 4] {
    let mean = |pts: &[[f32; 2]; 5]| {
        let n = pts.len() as f32;
        (
            pts.iter().map(|p| p[0]).sum::<f32>() / n,
            pts.iter().map(|p| p[1]).sum::<f32>() / n,
        )
    };
    let (sx, sy) = mean(src);
    let (dx, dy) = mean(dst);
    let (mut num_a, mut num_b, mut den) = (0.0f32, 0.0f32, 0.0f32);
    for (s, d) in src.iter().zip(dst) {
        let (xs, ys, xd, yd) = (s[0] - sx, s[1] - sy, d[0] - dx, d[1] - dy);
        num_a += xs * xd + ys * yd;
        num_b += xs * yd - ys * xd;
        den += xs * xs + ys * ys;
    }
    if den <= 0.0 {
        return [1.0, 0.0, dx - sx, dy - sy];
    }
    let (a, b) = (num_a / den, num_b / den);
    [a, b, dx - (a * sx - b * sy), dy - (b * sx + a * sy)]
}

/// The 112x112 ArcFace crop: every output pixel is sampled bilinearly from the inverse transform,
/// black outside the source.
fn align(image: &RgbImage, kps: &[[f32; 2]; 5]) -> RgbImage {
    let [a, b, tx, ty] = similarity(kps, &ARC_TEMPLATE);
    let det = a * a + b * b;
    let (w, h) = (image.width() as f32, image.height() as f32);
    RgbImage::from_fn(ARC_SIZE, ARC_SIZE, |u, v| {
        if det <= 0.0 {
            return image::Rgb([0, 0, 0]);
        }
        let (u, v) = (u as f32 - tx, v as f32 - ty);
        let x = (a * u + b * v) / det;
        let y = (-b * u + a * v) / det;
        if x < 0.0 || y < 0.0 || x > w - 1.0 || y > h - 1.0 {
            return image::Rgb([0, 0, 0]);
        }
        let (x0, y0) = (x.floor() as u32, y.floor() as u32);
        let (x1, y1) = (
            (x0 + 1).min(image.width() - 1),
            (y0 + 1).min(image.height() - 1),
        );
        let (fx, fy) = (x - x0 as f32, y - y0 as f32);
        let mut out = [0u8; 3];
        for (c, value) in out.iter_mut().enumerate() {
            let p = |px: u32, py: u32| f32::from(image.get_pixel(px, py)[c]);
            let top = p(x0, y0) * (1.0 - fx) + p(x1, y0) * fx;
            let bottom = p(x0, y1) * (1.0 - fx) + p(x1, y1) * fx;
            *value = (top * (1.0 - fy) + bottom * fy).round().clamp(0.0, 255.0) as u8;
        }
        image::Rgb(out)
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn face(score: f32, bbox: [f32; 4]) -> Detection {
        let [x1, y1, x2, y2] = bbox;
        let (w, h) = (x2 - x1, y2 - y1);
        Detection {
            score,
            bbox,
            kps: [
                [x1 + 0.3 * w, y1 + 0.4 * h],
                [x1 + 0.7 * w, y1 + 0.4 * h],
                [x1 + 0.5 * w, y1 + 0.6 * h],
                [x1 + 0.35 * w, y1 + 0.8 * h],
                [x1 + 0.65 * w, y1 + 0.8 * h],
            ],
        }
    }

    #[test]
    fn quality_gate() {
        let good = face(0.9, [100.0, 100.0, 300.0, 340.0]);
        assert_eq!(assess(std::slice::from_ref(&good), 640.0, 480.0), Ok(&good));
        assert_eq!(assess(&[], 640.0, 480.0), Err(FaceError::NoFace));
        let second = face(0.8, [400.0, 100.0, 520.0, 240.0]);
        assert_eq!(
            assess(&[good.clone(), second], 640.0, 480.0),
            Err(FaceError::MultipleFaces)
        );
        let tiny_background = face(0.8, [500.0, 10.0, 530.0, 40.0]);
        assert!(assess(&[good.clone(), tiny_background], 640.0, 480.0).is_ok());
        assert_eq!(
            assess(&[face(0.55, good.bbox)], 640.0, 480.0),
            Err(FaceError::Unclear)
        );
        assert_eq!(
            assess(&[face(0.9, [10.0, 10.0, 60.0, 70.0])], 640.0, 480.0),
            Err(FaceError::TooSmall)
        );
        assert_eq!(
            assess(&[face(0.9, [-60.0, 100.0, 140.0, 340.0])], 640.0, 480.0),
            Err(FaceError::OffCenter)
        );
        let mut rolled = good.clone();
        rolled.kps[1][1] += 120.0;
        assert_eq!(assess(&[rolled], 640.0, 480.0), Err(FaceError::Tilted));
        let mut turned = good.clone();
        turned.kps[2][0] += 50.0;
        assert_eq!(assess(&[turned], 640.0, 480.0), Err(FaceError::Tilted));
    }

    #[test]
    fn nms_keeps_the_best_of_overlapping_boxes() {
        let kept = nms(vec![
            face(0.7, [0.0, 0.0, 100.0, 100.0]),
            face(0.9, [5.0, 5.0, 105.0, 105.0]),
            face(0.8, [300.0, 300.0, 400.0, 400.0]),
        ]);
        let scores: Vec<f32> = kept.iter().map(|d| d.score).collect();
        assert_eq!(scores, vec![0.9, 0.8]);
    }

    #[test]
    fn decoding_follows_the_yunet_grid() {
        // 16x16 input, stride 8: 2x2 cells. Cell 2 is column 0, row 1.
        let mut cls = vec![0.0f32; 4];
        let mut obj = vec![0.0f32; 4];
        cls[2] = 0.81;
        obj[2] = 1.0;
        // Strong class score but weak objectness: sqrt(1 * 0.1) is below the threshold.
        cls[3] = 1.0;
        obj[3] = 0.1;
        let mut boxes = vec![0.0f32; 16];
        boxes[8..12].copy_from_slice(&[0.5, 0.5, 0.0, 0.0]);
        let kps = vec![0.5f32; 40];
        let level = |cls: &'static [f32]| Level {
            cls,
            obj: &[],
            boxes: &[],
            kps: &[],
            stride: 8,
            width: 16,
            height: 16,
        };
        let mut out = Vec::new();
        let full = Level {
            cls: &cls,
            obj: &obj,
            boxes: &boxes,
            kps: &kps,
            ..level(&[])
        };
        decode_level(full, 0.5, &mut out).expect("decode");
        assert_eq!(out.len(), 1);
        assert!((out[0].score - 0.9).abs() < 1e-6);
        // Centre (0.5 * 8, 1.5 * 8) = (4, 12), size exp(0) * 8 = 8, then divided by the scale.
        assert_eq!(out[0].bbox, [0.0, 16.0, 16.0, 32.0]);
        assert_eq!(out[0].kps[0], [8.0, 24.0]);

        let short = Level {
            obj: &obj,
            boxes: &boxes,
            kps: &kps,
            ..level(&[0.0; 3])
        };
        assert!(decode_level(short, 1.0, &mut out).is_err());
    }

    #[test]
    fn similarity_recovers_rotation_scale_and_shift() {
        let src = ARC_TEMPLATE;
        assert_eq!(
            similarity(&src, &src).map(|v| (v * 1000.0).round()),
            [1000.0, 0.0, 0.0, 0.0]
        );
        // dst = 2 * rot90(src) + (10, 20): a = 0, b = 2.
        let dst = src.map(|[x, y]| [-2.0 * y + 10.0, 2.0 * x + 20.0]);
        let [a, b, tx, ty] = similarity(&src, &dst);
        assert!(a.abs() < 1e-3 && (b - 2.0).abs() < 1e-3, "{a} {b}");
        assert!(
            (tx - 10.0).abs() < 1e-2 && (ty - 20.0).abs() < 1e-2,
            "{tx} {ty}"
        );
    }

    #[test]
    fn aligning_template_landmarks_is_identity() {
        let image = RgbImage::from_fn(112, 112, |x, y| image::Rgb([x as u8, y as u8, 7]));
        let crop = align(&image, &ARC_TEMPLATE);
        assert_eq!(crop.dimensions(), (112, 112));
        assert_eq!(crop.get_pixel(40, 60), &image::Rgb([40, 60, 7]));
    }

    #[test]
    fn letterbox_pads_the_short_side_and_feeds_bgr() {
        let image = RgbImage::from_pixel(200, 100, image::Rgb([250, 10, 0]));
        let (input, scale) = letterbox(&image, 64, 64);
        assert!((scale - 0.32).abs() < 1e-6);
        let plane = 64 * 64;
        assert_eq!(
            [input[0], input[plane], input[2 * plane]],
            [0.0, 10.0, 250.0]
        );
        assert_eq!(input[63 * 64], 0.0);
    }

    #[test]
    fn model_version_changes_with_either_model() {
        let base = model_version(b"det", b"emb");
        assert!(base.starts_with("yunet-") && base.len() <= 64, "{base}");
        assert_ne!(base, model_version(b"det2", b"emb"));
        assert_ne!(base, model_version(b"det", b"emb2"));
    }

    #[test]
    fn luma_reads_the_face_box() {
        let image = RgbImage::from_pixel(100, 100, image::Rgb([20, 20, 20]));
        assert!(mean_luma(&image, &[10.0, 10.0, 90.0, 90.0]) < MIN_LUMA);
        let image = RgbImage::from_pixel(100, 100, image::Rgb([128, 128, 128]));
        assert!((mean_luma(&image, &[-5.0, -5.0, 200.0, 200.0]) - 128.0).abs() < 1.0);
    }
}
