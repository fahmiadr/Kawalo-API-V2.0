//! Ogg/Opus recordings to 16 kHz mono PCM, the WAV file whisper.cpp reads, and the energy gate
//! that keeps silence away from Whisper (which hallucinates text on non-speech audio).

use std::fs::OpenOptions;
use std::io::{self, Cursor, Write};
use std::os::unix::fs::OpenOptionsExt;
use std::path::Path;

use ogg::PacketReader;

use crate::features::ptt::recording::{Decoder, OpusError};

pub const SAMPLE_RATE: u32 = 16_000;
/// Decoding stops here. A turn is at most 30 s, so this only bounds a foreign or corrupt file.
const MAX_SAMPLES: usize = SAMPLE_RATE as usize * 120;
/// The gate looks at 20 ms frames.
const GATE_FRAME: usize = SAMPLE_RATE as usize / 50;

#[derive(Debug, thiserror::Error)]
pub enum DecodeError {
    #[error("ogg: {0}")]
    Ogg(#[from] ogg::OggReadError),
    #[error("not an Ogg/Opus stream")]
    NotOpus,
    #[error(transparent)]
    Opus(#[from] OpusError),
}

/// Decodes the first logical Opus stream, dropping the RFC 7845 pre-skip and trimming the end to
/// the last granule position, so the result has the duration that was recorded.
pub fn decode(file: &[u8]) -> Result<Vec<i16>, DecodeError> {
    let mut reader = PacketReader::new(Cursor::new(file));
    let head = reader.read_packet()?.ok_or(DecodeError::NotOpus)?;
    if head.data.len() < 19 || !head.data.starts_with(b"OpusHead") {
        return Err(DecodeError::NotOpus);
    }
    let serial = head.stream_serial();
    // Pre-skip and granules count 48 kHz samples; the decoder outputs 16 kHz.
    let pre_skip_48k = u64::from(u16::from_le_bytes([head.data[10], head.data[11]]));
    let mut decoder = Decoder::new(SAMPLE_RATE as i32)?;
    let mut pcm = Vec::new();
    let mut end_48k = None;
    while let Some(packet) = reader.read_packet()? {
        if packet.stream_serial() != serial || packet.data.starts_with(b"OpusTags") {
            continue;
        }
        decoder.decode(&packet.data, &mut pcm)?;
        if packet.last_in_page() {
            end_48k = Some(packet.absgp_page());
        }
        if pcm.len() >= MAX_SAMPLES {
            break;
        }
    }
    let skip = usize::try_from(pre_skip_48k / 3).unwrap_or(usize::MAX);
    pcm.drain(..skip.min(pcm.len()));
    if let Some(end) = end_48k {
        let playable = usize::try_from(end.saturating_sub(pre_skip_48k) / 3).unwrap_or(usize::MAX);
        pcm.truncate(playable);
    }
    pcm.truncate(MAX_SAMPLES);
    Ok(pcm)
}

/// Milliseconds of 20 ms frames whose RMS level reaches `threshold_dbfs` (0 dBFS = full scale).
/// Leading, trailing and in-between pauses do not count, so a 10 s turn with half a second of
/// words measures 500.
pub fn speech_ms(pcm: &[i16], threshold_dbfs: f32) -> u64 {
    let rms = 32768.0 * 10f64.powf(f64::from(threshold_dbfs) / 20.0);
    let min_energy = rms * rms * GATE_FRAME as f64;
    let loud = pcm
        .as_chunks::<GATE_FRAME>()
        .0
        .iter()
        .filter(|frame| {
            frame
                .iter()
                .map(|&s| f64::from(s) * f64::from(s))
                .sum::<f64>()
                >= min_energy
        })
        .count();
    loud as u64 * 20
}

/// Writes a 16-bit mono PCM WAV readable only by the owner. Fails if the file exists.
pub fn write_wav(path: &Path, pcm: &[i16]) -> io::Result<()> {
    let data_len = u32::try_from(pcm.len() * 2)
        .ok()
        .filter(|n| *n <= u32::MAX - 36)
        .ok_or_else(|| io::Error::other("audio too long for WAV"))?;
    let mut out = Vec::with_capacity(44 + pcm.len() * 2);
    out.extend_from_slice(b"RIFF");
    out.extend_from_slice(&(36 + data_len).to_le_bytes());
    out.extend_from_slice(b"WAVEfmt ");
    out.extend_from_slice(&16u32.to_le_bytes()); // fmt chunk size
    out.extend_from_slice(&1u16.to_le_bytes()); // PCM
    out.extend_from_slice(&1u16.to_le_bytes()); // mono
    out.extend_from_slice(&SAMPLE_RATE.to_le_bytes());
    out.extend_from_slice(&(SAMPLE_RATE * 2).to_le_bytes()); // byte rate
    out.extend_from_slice(&2u16.to_le_bytes()); // block align
    out.extend_from_slice(&16u16.to_le_bytes()); // bits per sample
    out.extend_from_slice(b"data");
    out.extend_from_slice(&data_len.to_le_bytes());
    for sample in pcm {
        out.extend_from_slice(&sample.to_le_bytes());
    }
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .mode(0o600)
        .open(path)?;
    file.write_all(&out)
}

#[cfg(test)]
mod tests {
    use std::os::unix::fs::PermissionsExt;

    use super::*;
    use crate::features::ptt::recording::{Config, TurnFrame, encode_turn};

    fn duration_ms(pcm: &[i16]) -> u64 {
        pcm.len() as u64 * 1000 / u64::from(SAMPLE_RATE)
    }

    fn tone(ms: usize, amplitude: f64) -> Vec<i16> {
        (0..ms * 16)
            .map(|n| {
                (amplitude * (n as f64 * 440.0 * std::f64::consts::TAU / 16_000.0).sin()) as i16
            })
            .collect()
    }

    fn encode(pcm: &[i16]) -> Vec<u8> {
        let payloads: Vec<Vec<u8>> = pcm
            .chunks(320)
            .map(|c| {
                let mut frame = [0i16; 320];
                frame[..c.len()].copy_from_slice(c);
                frame.iter().flat_map(|s| s.to_le_bytes()).collect()
            })
            .collect();
        let frames = payloads.iter().enumerate().map(|(i, p)| TurnFrame {
            sequence: i as u16,
            offset_ms: i as u32 * 20,
            payload: p,
        });
        encode_turn(frames, 7, Config::default()).unwrap()
    }

    #[test]
    fn decoding_restores_the_recorded_duration() {
        for ms in [20, 1_000, 2_340, 30_000] {
            let decoded = decode(&encode(&tone(ms, 8_000.0))).unwrap();
            let got = duration_ms(&decoded) as i64;
            assert!((got - ms as i64).abs() <= 20, "{ms} ms decoded as {got} ms");
        }
    }

    #[test]
    fn decoded_audio_keeps_its_level() {
        let decoded = decode(&encode(&tone(2_000, 8_000.0))).unwrap();
        assert!(speech_ms(&decoded, -40.0) >= 1_900);
        let silent = decode(&encode(&vec![0; 32_000])).unwrap();
        assert_eq!(speech_ms(&silent, -40.0), 0);
    }

    #[test]
    fn gate_counts_only_loud_frames() {
        let mut pcm = vec![0i16; 16_000];
        pcm.extend(tone(500, 8_000.0));
        pcm.extend(tone(500, 50.0)); // about -56 dBFS: room noise, not speech
        assert_eq!(speech_ms(&pcm, -40.0), 500);
    }

    #[test]
    fn garbage_is_not_opus() {
        assert!(decode(b"not an ogg file at all").is_err());
        assert!(decode(b"").is_err());
    }

    #[test]
    fn wav_has_a_valid_header_and_owner_only_permissions() {
        let dir = tempfile::tempdir().unwrap();
        let path = dir.path().join("a.wav");
        write_wav(&path, &[1, -1, 2]).unwrap();
        let bytes = std::fs::read(&path).unwrap();
        assert_eq!(bytes.len(), 44 + 6);
        assert_eq!(&bytes[..4], b"RIFF");
        assert_eq!(u32::from_le_bytes(bytes[4..8].try_into().unwrap()), 42);
        assert_eq!(&bytes[8..16], b"WAVEfmt ");
        assert_eq!(
            u32::from_le_bytes(bytes[24..28].try_into().unwrap()),
            16_000
        );
        assert_eq!(u32::from_le_bytes(bytes[40..44].try_into().unwrap()), 6);
        assert_eq!(&bytes[44..46], &1i16.to_le_bytes());
        let mode = std::fs::metadata(&path).unwrap().permissions().mode();
        assert_eq!(mode & 0o777, 0o600);
        assert!(write_wav(&path, &[0]).is_err(), "never overwrites");
    }
}
