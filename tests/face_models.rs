//! The real YuNet + ArcFace pipeline on real photos. Ignored by default because the weights are
//! never in the repository. Run with:
//!
//! ```text
//! FACE_ORT_LIBRARY=/path/libonnxruntime.dylib FACE_DETECTOR_MODEL=/path/yunet.onnx \
//! FACE_EMBEDDER_MODEL=/path/glintr100.onnx FACE_SAMPLE_DIR=/path/samples \
//! cargo test --release --test face_models -- --ignored --nocapture
//! ```
//!
//! `FACE_SAMPLE_DIR` holds one folder per person with JPEGs, at least two people. An optional
//! `reference.json` (`{"person/file.jpg": [f32; 512]}`) from another implementation of the same
//! pipeline is checked for cosine > 0.999 on every photo that passed the quality gate. Save the
//! samples as 4:4:4 quality-100 JPEGs: with chroma subsampling, the decoders' upsampling differs
//! by a level or two per pixel, which alone moves the cosine to about 0.998.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::time::Instant;

use kawalo_api::core::config::FaceConfig;
use kawalo_api::core::face::{FaceError, Faces, cosine};

fn jpegs(dir: &Path) -> Vec<PathBuf> {
    let mut files: Vec<PathBuf> = std::fs::read_dir(dir)
        .expect("read sample dir")
        .filter_map(|e| e.ok().map(|e| e.path()))
        .collect();
    files.sort();
    files
}

#[tokio::test]
#[ignore = "needs FACE_ORT_LIBRARY, FACE_DETECTOR_MODEL, FACE_EMBEDDER_MODEL and FACE_SAMPLE_DIR"]
async fn real_models_tell_people_apart() {
    let Ok(sample_dir) = std::env::var("FACE_SAMPLE_DIR") else {
        eprintln!("FACE_SAMPLE_DIR is unset; skipping");
        return;
    };
    let config = FaceConfig {
        encryption_key: Some([7; 32]),
        ..FaceConfig::from_env()
    };
    if config.ort_library.is_none()
        || config.detector_model.is_none()
        || config.embedder_model.is_none()
    {
        eprintln!("FACE_ORT_LIBRARY / FACE_DETECTOR_MODEL / FACE_EMBEDDER_MODEL unset; skipping");
        return;
    }
    let faces = Faces::load(&config);
    let ready = faces.ready().expect("models load");
    eprintln!("model version {}", ready.model_version());

    let root = Path::new(&sample_dir);
    let mut embeddings: BTreeMap<String, (String, Vec<f32>)> = BTreeMap::new();
    for person in jpegs(root).into_iter().filter(|p| p.is_dir()) {
        let name = person
            .file_name()
            .map(|n| n.to_string_lossy().into_owned())
            .expect("person name");
        for file in jpegs(&person) {
            let key = format!(
                "{name}/{}",
                file.file_name().expect("file name").to_string_lossy()
            );
            let bytes = std::fs::read(&file).expect("read photo");
            let started = Instant::now();
            match ready.embed_photo(bytes).await {
                Ok((_, embedding)) => {
                    eprintln!("{key}: {:?}", started.elapsed());
                    embeddings.insert(key, (name.clone(), embedding));
                }
                // Arbitrary sample photos may fail the quality gate; only engine errors are bugs.
                Err(FaceError::Engine(cause)) => panic!("{key}: {cause}"),
                Err(err) => eprintln!("{key}: rejected, {}", err.message()),
            }
        }
    }

    let (mut same_min, mut diff_max) = (f32::MAX, f32::MIN);
    let all: Vec<_> = embeddings.values().collect();
    for (i, (a_name, a)) in all.iter().enumerate() {
        for (b_name, b) in &all[i + 1..] {
            let score = cosine(a, b);
            if a_name == b_name {
                same_min = same_min.min(score);
            } else {
                diff_max = diff_max.max(score);
            }
        }
    }
    eprintln!("same-person min {same_min:.3}, different-person max {diff_max:.3}");
    assert!(
        same_min < f32::MAX && diff_max > f32::MIN,
        "need 2+ people with 2+ photos"
    );
    assert!(same_min > diff_max, "{same_min} <= {diff_max}");

    let reference = root.join("reference.json");
    if let Ok(raw) = std::fs::read(&reference) {
        let expected: BTreeMap<String, Vec<f32>> =
            serde_json::from_slice(&raw).expect("reference.json");
        for (key, vector) in expected {
            let Some((_, actual)) = embeddings.get(&key) else {
                continue;
            };
            let score = cosine(actual, &vector);
            eprintln!("reference {key}: cosine {score:.5}");
            assert!(score > 0.999, "{key}: {score}");
        }
    }
}
