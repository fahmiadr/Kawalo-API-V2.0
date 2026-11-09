//! Fan-out latency benchmark:
//! `cargo test --test ptt_bench -- --ignored --nocapture` (add `--release` for real numbers).

mod common;
mod ptt_support;

use std::time::{Duration, Instant};

use common::TestApp;
use ptt_support::*;
use serde_json::json;
use tokio_tungstenite::tungstenite::Message;

const LISTENERS: usize = 100;
const FRAMES: usize = 500;
const STAMP: std::ops::Range<usize> = 7..15;

#[tokio::test]
#[ignore = "benchmark, prints numbers"]
async fn fan_out_latency() {
    let app = TestApp::start_with(|config| config.rate_limit_max = 100_000).await;
    let addr = app.serve().await;
    let squad = Squad::new();

    let token = squad.token(&app, "speaker", false).await;
    let mut speaker = join(addr, &squad.group, &token).await;
    speaker.json().await;
    let mut listeners = Vec::new();
    for i in 0..LISTENERS {
        let token = squad.token(&app, &format!("l{i}"), false).await;
        let mut ws = join(addr, &squad.group, &token).await;
        ws.json().await;
        listeners.push(ws);
    }
    speaker
        .send(&json!({"type": "FLOOR_REQUEST", "requestId": "b"}))
        .await;
    speaker.json().await;

    let start = Instant::now();
    let micros = move || u64::try_from(start.elapsed().as_micros()).expect("fits");
    let mut readers = Vec::new();
    for mut ws in listeners {
        readers.push(tokio::spawn(async move {
            let mut latencies = Vec::with_capacity(FRAMES);
            while latencies.len() < FRAMES {
                if let Message::Binary(frame) = ws.next_message().await {
                    let sent = u64::from_be_bytes(frame[STAMP].try_into().expect("stamp"));
                    latencies.push(micros().saturating_sub(sent));
                }
            }
            latencies
        }));
    }
    for _ in 0..FRAMES {
        let mut frame = opus(0, 100);
        frame[STAMP].copy_from_slice(&micros().to_be_bytes());
        speaker.audio(&frame).await;
        tokio::time::sleep(Duration::from_millis(20)).await;
    }

    let mut all: Vec<u64> = Vec::new();
    for reader in readers {
        all.extend(reader.await.expect("reader"));
    }
    all.sort_unstable();
    let pick = |p: f64| all[(all.len() as f64 * p) as usize - 1];
    println!(
        "{LISTENERS} listeners x {FRAMES} frames: p50 {}us p99 {}us max {}us",
        pick(0.5),
        pick(0.99),
        all[all.len() - 1]
    );
}
