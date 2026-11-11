use std::io::Cursor;

use ogg::PacketReader;

use super::opus::{Decoder, Encoder, EncoderSettings, FRAME_SAMPLES};
use super::*;

const SERIAL: u32 = 0x4b41_5741;

/// Synthetic speech-like PCM, not a recording: 220 Hz and 1.1 kHz tones with a syllable-rate
/// amplitude envelope.
fn synthetic_pcm_frame(frame_index: usize) -> Vec<u8> {
    let mut out = Vec::with_capacity(PCM_PAYLOAD_BYTES);
    for i in 0..FRAME_SAMPLES {
        let t = (frame_index * FRAME_SAMPLES + i) as f32 / 16_000.0;
        let envelope = 0.5 + 0.5 * (2.0 * std::f32::consts::PI * 3.0 * t).sin();
        let s = envelope
            * (0.5 * (2.0 * std::f32::consts::PI * 220.0 * t).sin()
                + 0.25 * (2.0 * std::f32::consts::PI * 1100.0 * t).sin());
        out.extend_from_slice(&((s * 12_000.0) as i16).to_le_bytes());
    }
    out
}

fn frames_for(count: usize) -> Vec<(u16, u32, Vec<u8>)> {
    (0..count)
        .map(|i| (i as u16, (i * 20) as u32, synthetic_pcm_frame(i)))
        .collect()
}

fn encode(frames: &[(u16, u32, Vec<u8>)], cfg: Config) -> Vec<u8> {
    encode_turn(
        frames.iter().map(|(s, o, p)| TurnFrame {
            sequence: *s,
            offset_ms: *o,
            payload: p,
        }),
        SERIAL,
        cfg,
    )
    .unwrap()
}

struct Parsed {
    head: Vec<u8>,
    tags: Vec<u8>,
    packets: Vec<Vec<u8>>,
    /// (granule, is_last_in_stream) per audio page.
    pages: Vec<(u64, bool)>,
    first_in_stream: bool,
}

fn parse(bytes: &[u8]) -> Parsed {
    let mut reader = PacketReader::new(Cursor::new(bytes));
    let mut all = Vec::new();
    while let Some(p) = reader.read_packet().unwrap() {
        all.push(p);
    }
    let first_in_stream = all[0].first_in_stream();
    let mut pages = Vec::new();
    let mut packets = Vec::new();
    for p in &all[2..] {
        packets.push(p.data.clone());
        if p.last_in_page() {
            pages.push((p.absgp_page(), p.last_in_stream()));
        }
    }
    Parsed {
        head: all[0].data.clone(),
        tags: all[1].data.clone(),
        packets,
        pages,
        first_in_stream,
    }
}

fn decode_all(packets: &[Vec<u8>], rate: i32) -> Vec<i16> {
    let mut dec = Decoder::new(rate).unwrap();
    let mut pcm = Vec::new();
    for p in packets {
        dec.decode(p, &mut pcm).unwrap();
    }
    pcm
}

fn pre_skip(head: &[u8]) -> usize {
    u16::from_le_bytes([head[10], head[11]]) as usize
}

fn rms(pcm: &[i16]) -> f64 {
    (pcm.iter().map(|&s| f64::from(s).powi(2)).sum::<f64>() / pcm.len() as f64).sqrt()
}

fn original_pcm(frames: &[(u16, u32, Vec<u8>)]) -> Vec<i16> {
    frames
        .iter()
        .flat_map(|(_, _, p)| p.as_chunks::<2>().0.iter().map(|b| i16::from_le_bytes(*b)))
        .collect()
}

#[test]
fn round_trip_keeps_duration_and_energy() {
    let frames = frames_for(250); // 5 s
    let parsed = parse(&encode(&frames, Config::default()));
    let skip = pre_skip(&parsed.head);
    let decoded = decode_all(&parsed.packets, 16_000);
    // At 16 kHz one sample is 3 granule units, so pre-skip is skip / 3 decoded samples.
    assert!(
        decoded.len() >= skip / 3 + 250 * FRAME_SAMPLES,
        "all audio survives the delay"
    );
    let granule = parsed.pages.last().unwrap().0 as usize;
    assert_eq!(
        granule - skip,
        250 * 960,
        "end granule trims the flush frames"
    );

    let original = original_pcm(&frames);
    let decoded = &decoded[skip / 3..skip / 3 + original.len()];
    let ratio = rms(decoded) / rms(&original);
    assert!((0.7..1.3).contains(&ratio), "energy ratio {ratio}");
}

#[test]
fn ogg_structure_follows_rfc_7845() {
    let parsed = parse(&encode(&frames_for(260), Config::default()));
    assert!(parsed.first_in_stream);
    assert_eq!(&parsed.head[..8], b"OpusHead");
    assert_eq!(parsed.head[8], 1, "version");
    assert_eq!(parsed.head[9], 1, "mono");
    assert_eq!(
        u32::from_le_bytes(parsed.head[12..16].try_into().unwrap()),
        16_000
    );
    assert_eq!(
        i16::from_le_bytes([parsed.head[16], parsed.head[17]]),
        0,
        "gain"
    );
    assert_eq!(parsed.head[18], 0, "mapping family");
    let skip = pre_skip(&parsed.head);
    assert!(
        skip > 0 && skip.is_multiple_of(3),
        "pre-skip {skip} is 3x the 16 kHz lookahead"
    );
    assert_eq!(&parsed.tags[..8], b"OpusTags");

    // 260 audio frames + flush frames, 50 per page.
    let total = parsed.packets.len();
    assert_eq!(parsed.pages.len(), total.div_ceil(50));
    let mut previous = 0;
    for (i, (granule, last)) in parsed.pages.iter().enumerate() {
        assert!(*granule > previous);
        previous = *granule;
        assert_eq!(
            *last,
            i == parsed.pages.len() - 1,
            "EOS only on the final page"
        );
        if i + 1 < parsed.pages.len() {
            assert_eq!(*granule, (i as u64 + 1) * 50 * 960);
        }
    }
    assert_eq!(previous, skip as u64 + 260 * 960);
}

#[test]
fn missing_frames_are_filled_with_silence() {
    let mut frames = frames_for(100);
    frames.drain(40..60); // 20 frames = 400 ms lost, sequence and offset both jump
    let parsed = parse(&encode(&frames, Config::default()));
    let skip = pre_skip(&parsed.head);
    let granule = parsed.pages.last().unwrap().0 as usize;
    assert_eq!(
        granule - skip,
        100 * 960,
        "duration is not shortened by the loss"
    );

    let decoded = decode_all(&parsed.packets, 16_000);
    let hole = &decoded[skip / 3 + 45 * FRAME_SAMPLES..skip / 3 + 55 * FRAME_SAMPLES];
    let around = &decoded[skip / 3 + 10 * FRAME_SAMPLES..skip / 3 + 30 * FRAME_SAMPLES];
    assert!(
        rms(hole) < rms(around) * 0.05,
        "hole {} vs {}",
        rms(hole),
        rms(around)
    );
}

#[test]
fn offset_pause_without_sequence_gap_is_filled() {
    // A sender that stops transmitting during silence keeps counting sequence numbers.
    let frames: Vec<_> = frames_for(100)
        .into_iter()
        .map(|(s, o, p)| (s, if s >= 50 { o + 1_000 } else { o }, p))
        .collect();
    let parsed = parse(&encode(&frames, Config::default()));
    let granule = parsed.pages.last().unwrap().0 as usize;
    assert_eq!(granule - pre_skip(&parsed.head), (100 + 50) * 960);
}

#[test]
fn offset_jitter_and_late_bursts_do_not_change_the_timeline() {
    let jittered: Vec<_> = frames_for(100)
        .into_iter()
        .map(|(s, o, p)| {
            // +-15 ms jitter and a stalled read thread: frames 30..34 arrive stamped 80 ms late
            // with their offsets bunched up.
            let o = match s {
                30..=34 => 30 * 20 + 80 + (s as u32 - 30),
                _ => (o as i32 + [0, 9, -7, 15, -12][s as usize % 5]).max(0) as u32,
            };
            (s, o, p)
        })
        .collect();
    let parsed = parse(&encode(&jittered, Config::default()));
    let granule = parsed.pages.last().unwrap().0 as usize;
    assert_eq!(granule - pre_skip(&parsed.head), 100 * 960);
}

#[test]
fn duplicates_and_out_of_order_frames_are_dropped() {
    let frames = frames_for(10);
    let mut turn = TurnEncoder::new(Vec::new(), SERIAL, Config::default()).unwrap();
    let push = |t: &mut TurnEncoder<Vec<u8>>, i: usize| {
        let (s, o, p) = &frames[i];
        t.push(TurnFrame {
            sequence: *s,
            offset_ms: *o,
            payload: p,
        })
        .unwrap()
    };
    assert_eq!(push(&mut turn, 0), Push::Accepted);
    assert_eq!(push(&mut turn, 1), Push::Accepted);
    assert_eq!(push(&mut turn, 1), Push::Duplicate);
    assert_eq!(push(&mut turn, 3), Push::Accepted); // frame 2 skipped: silence inserted
    assert_eq!(push(&mut turn, 2), Push::Duplicate); // late arrival of the skipped frame
    assert_eq!(push(&mut turn, 4), Push::Accepted);
    assert_eq!(turn.accepted_frames(), 4);
    assert_eq!(turn.duration_ms(), 5 * 20);
}

#[test]
fn sequence_wraparound_is_continuous() {
    let mut turn = TurnEncoder::new(Vec::new(), SERIAL, Config::default()).unwrap();
    let pcm = synthetic_pcm_frame(0);
    for (i, seq) in [65_534u16, 65_535, 0, 1].into_iter().enumerate() {
        let r = turn
            .push(TurnFrame {
                sequence: seq,
                offset_ms: i as u32 * 20,
                payload: &pcm,
            })
            .unwrap();
        assert_eq!(r, Push::Accepted);
    }
    assert_eq!(turn.duration_ms(), 80);
}

#[test]
fn non_640_payloads_pass_through_unchanged() {
    let mut enc = Encoder::new(EncoderSettings {
        bitrate_bps: 24_000,
        complexity: 5,
        vbr: true,
        dtx: false,
    })
    .unwrap();
    let packets: Vec<Vec<u8>> = (0..30)
        .map(|i| {
            let pcm: [i16; FRAME_SAMPLES] = synthetic_pcm_frame(i)
                .as_chunks::<2>()
                .0
                .iter()
                .map(|b| i16::from_le_bytes(*b))
                .collect::<Vec<_>>()
                .try_into()
                .unwrap();
            enc.encode(&pcm).unwrap()
        })
        .collect();
    assert!(packets.iter().all(|p| p.len() != PCM_PAYLOAD_BYTES));

    let bytes = encode_turn(
        packets.iter().enumerate().map(|(i, p)| TurnFrame {
            sequence: i as u16,
            offset_ms: i as u32 * 20,
            payload: p,
        }),
        SERIAL,
        Config::default(),
    )
    .unwrap();
    let parsed = parse(&bytes);
    assert_eq!(parsed.packets, packets, "bytes are stored as received");
    assert_eq!(parsed.pages.last().unwrap(), &(30 * 960, true));
}

#[test]
fn unparsable_payload_is_rejected_without_touching_state() {
    let mut turn = TurnEncoder::new(Vec::new(), SERIAL, Config::default()).unwrap();
    for payload in [&[][..], &[0xff, 0xff][..]] {
        let r = turn
            .push(TurnFrame {
                sequence: 0,
                offset_ms: 0,
                payload,
            })
            .unwrap();
        assert_eq!(r, Push::Invalid);
    }
    assert!(matches!(turn.finish(), Err(Error::Empty)));
}

#[test]
fn duration_cap_stops_a_hostile_sequence_jump() {
    let mut turn = TurnEncoder::new(Vec::new(), SERIAL, Config::default()).unwrap();
    let pcm = synthetic_pcm_frame(0);
    let push = |t: &mut TurnEncoder<Vec<u8>>, sequence, offset_ms| {
        t.push(TurnFrame {
            sequence,
            offset_ms,
            payload: &pcm,
        })
        .unwrap()
    };
    assert_eq!(push(&mut turn, 0, 0), Push::Accepted);
    assert_eq!(push(&mut turn, 30_000, 0), Push::Full);
    assert_eq!(push(&mut turn, 1, u32::MAX), Push::Full);
    assert_eq!(turn.duration_ms(), 20, "rejected frames leave no trace");
    let bytes = turn.finish().unwrap();
    assert_eq!(parse(&bytes).pages.last().unwrap().0 as usize - 312, 960);
}

#[test]
fn a_full_30_second_turn_fits_the_cap() {
    let frames = frames_for(1_500);
    let parsed = parse(&encode(&frames, Config::default()));
    assert_eq!(
        parsed.pages.last().unwrap().0 as usize - pre_skip(&parsed.head),
        1_500 * 960
    );
}

#[test]
fn empty_turn_is_an_error() {
    let turn = TurnEncoder::new(Vec::new(), SERIAL, Config::default()).unwrap();
    assert!(matches!(turn.finish(), Err(Error::Empty)));
}

/// Prints the numbers quoted in the spike report; run with `--ignored --nocapture`.
#[test]
#[ignore = "measurement, not a check"]
fn measure_sizes_and_speed() {
    let frames = frames_for(1_500); // 30 s
    for (label, bitrate, dtx) in [
        ("16k", 16_000, false),
        ("24k", 24_000, false),
        ("16k+dtx", 16_000, true),
    ] {
        for per_page in [1, 50] {
            let cfg = Config {
                bitrate_bps: bitrate,
                dtx,
                packets_per_page: per_page,
                ..Config::default()
            };
            let start = std::time::Instant::now();
            let bytes = encode(&frames, cfg);
            let ms = start.elapsed().as_secs_f64() * 1000.0;
            let parsed = parse(&bytes);
            let payload: usize = parsed.packets.iter().map(Vec::len).sum();
            println!(
                "{label} packets/page={per_page}: total={} B ({:.0} B/s), opus payload={payload} B, ogg overhead={:.1}% of file, pages={}, encode={ms:.1} ms",
                bytes.len(),
                bytes.len() as f64 / 30.0,
                100.0 * (bytes.len() - payload) as f64 / bytes.len() as f64,
                parsed.pages.len() + 2,
            );
        }
    }
}

/// Writes the 10 s demo file used for the ffprobe/ffmpeg check; run with `--ignored`.
#[test]
#[ignore = "writes files into SPIKE_OUT_DIR"]
fn write_demo_files() {
    let dir = std::path::PathBuf::from(std::env::var("SPIKE_OUT_DIR").unwrap());
    let frames: Vec<_> = frames_for(500) // 10 s with pauses: 2 s of silence at 3..5 s, 1 s lost at 7 s
        .into_iter()
        .filter(|(s, _, _)| !(150..250).contains(s) && !(350..400).contains(s))
        .collect();
    std::fs::write(
        dir.join("demo_24k.opus"),
        encode(&frames, Config::default()),
    )
    .unwrap();
    std::fs::write(
        dir.join("demo_16k.opus"),
        encode(
            &frames,
            Config {
                bitrate_bps: 16_000,
                ..Config::default()
            },
        ),
    )
    .unwrap();
    let pcm: Vec<u8> = frames_for(500)
        .into_iter()
        .flat_map(|(_, _, p)| p)
        .collect();
    std::fs::write(dir.join("demo_source.s16le"), pcm).unwrap();
}

/// Encodes a raw s16le 16 kHz mono file (`SPIKE_PCM_IN`) the way the session would see it, with
/// jitter-free offsets; writes `SPIKE_OPUS_OUT`.
#[test]
#[ignore = "reads and writes files named by environment variables"]
fn encode_pcm_file() {
    let pcm = std::fs::read(std::env::var("SPIKE_PCM_IN").unwrap()).unwrap();
    let bitrate = std::env::var("SPIKE_BITRATE").map_or(24_000, |v| v.parse().unwrap());
    let frames: Vec<_> = pcm
        .chunks_exact(PCM_PAYLOAD_BYTES)
        .enumerate()
        .map(|(i, p)| (i as u16, (i * 20) as u32, p.to_vec()))
        .collect();
    let cfg = Config {
        bitrate_bps: bitrate,
        dtx: std::env::var("SPIKE_DTX").is_ok(),
        complexity: std::env::var("SPIKE_COMPLEXITY").map_or(5, |v| v.parse().unwrap()),
        max_duration_ms: 120_000,
        ..Config::default()
    };
    std::fs::write(
        std::env::var("SPIKE_OPUS_OUT").unwrap(),
        encode(&frames, cfg),
    )
    .unwrap();
}
