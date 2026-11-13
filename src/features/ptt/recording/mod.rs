//! Turns one push-to-talk speaker turn into an Ogg/Opus file (RFC 7845).
//!
//! The app sends raw 16 kHz mono 16-bit PCM, 640 bytes per 20 ms frame, behind a 7-byte header
//! (`0x10`, u16 sequence, u32 offset in ms since the burst started). Frames are encoded here with
//! libopus. A payload of any other length is taken to be an Opus packet from a future app version
//! and is muxed as is.
//!
//! Timeline rules (the audio position is tracked in 48 kHz samples, the Ogg granule unit):
//! - The sequence number detects loss: a jump of `n` fills `n` frames of silence, a repeat or
//!   older number is dropped (over one TCP socket order is guaranteed, so this is defensive).
//! - The offset detects pauses: if it is further ahead of the audio position than
//!   `gap_tolerance_ms`, the difference is filled with silence in whole 20 ms frames. An offset
//!   that is behind the audio position is ignored, never a reason to drop audio: the app stamps
//!   the wall-clock time a frame was read, and a stalled read thread delivers its backlog in a
//!   burst of contiguous samples.
//! - The turn is capped at `max_duration_ms`, which also bounds what a hostile sequence or offset
//!   can make the encoder emit.
//!
//! Passthrough packets are assumed to carry 20 ms for sequence-gap accounting.
//!
//! Alongside the audio the encoder keeps one loudness value per 20 ms frame, from which
//! [`TurnEncoder::waveform`] derives the bars the app draws. Passthrough packets are never
//! decoded, so a turn that contains any has no waveform.

mod opus;

use std::io::{self, Write};

use ogg::{PacketWriteEndInfo, PacketWriter};

pub use opus::{Decoder, OpusError};
use opus::{Encoder, EncoderSettings, FRAME_SAMPLES, SAMPLE_RATE};

/// One frame lasts 20 ms: 960 samples at the 48 kHz granule rate.
const FRAME_48K: u64 = 960;
const PCM_PAYLOAD_BYTES: usize = FRAME_SAMPLES * 2;

/// Bars per waveform. 48 bytes are 64 base64 characters without padding on the wire.
pub const WAVEFORM_BARS: usize = 48;

/// Frame RMS that maps to a full bar when nothing in the turn is louder. Without this floor a
/// turn of background hiss would be stretched to look like speech.
const WAVEFORM_FULL_SCALE_FLOOR: u16 = 1_000;

#[derive(Debug, Clone)]
pub struct Config {
    pub bitrate_bps: i32,
    pub complexity: i32,
    pub vbr: bool,
    pub dtx: bool,
    /// Audio packets per Ogg page; 50 is about one second at 20 ms.
    pub packets_per_page: usize,
    pub max_duration_ms: u32,
    pub gap_tolerance_ms: u32,
}

impl Default for Config {
    fn default() -> Self {
        Self {
            bitrate_bps: 24_000,
            complexity: 5,
            vbr: true,
            dtx: false,
            packets_per_page: 50,
            max_duration_ms: 30_000,
            gap_tolerance_ms: 100,
        }
    }
}

#[derive(Debug, thiserror::Error)]
pub enum Error {
    #[error(transparent)]
    Opus(#[from] opus::OpusError),
    #[error("ogg write failed: {0}")]
    Io(#[from] io::Error),
    #[error("turn has no audio")]
    Empty,
}

/// What [`TurnEncoder::push`] did with a frame.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Push {
    Accepted,
    /// Sequence number repeated or older than the last accepted frame.
    Duplicate,
    /// Not 640 bytes and not a parsable Opus packet.
    Invalid,
    /// The frame (with any silence before it) would pass `max_duration_ms`; the recorder should
    /// finish the turn.
    Full,
}

#[derive(Debug, Clone, Copy)]
pub struct TurnFrame<'a> {
    pub sequence: u16,
    pub offset_ms: u32,
    /// Payload after the 7-byte header.
    pub payload: &'a [u8],
}

pub struct TurnEncoder<W: Write> {
    cfg: Config,
    enc: Encoder,
    writer: PacketWriter<'static, W>,
    serial: u32,
    pre_skip: u64,
    next_seq: Option<u16>,
    /// Duration of everything emitted so far, in 48 kHz samples.
    position: u64,
    page_packets: usize,
    /// The newest packet is held back so the last one can carry the end-of-stream flag.
    held: Option<(Vec<u8>, u64)>,
    encoded_pcm: bool,
    accepted: u32,
    /// RMS per emitted 20 ms frame, silence fill included; `None` once a passthrough packet
    /// arrives.
    levels: Option<Vec<u16>>,
}

impl<W: Write> TurnEncoder<W> {
    /// Writes the OpusHead and OpusTags pages immediately. `serial` identifies the logical
    /// stream; any value works for a single-stream file.
    pub fn new(sink: W, serial: u32, cfg: Config) -> Result<Self, Error> {
        let mut enc = Encoder::new(EncoderSettings {
            bitrate_bps: cfg.bitrate_bps,
            complexity: cfg.complexity,
            vbr: cfg.vbr,
            dtx: cfg.dtx,
        })?;
        // RFC 7845 counts pre-skip in 48 kHz samples.
        let pre_skip = (enc.lookahead()? as u64) * 3;
        let mut writer = PacketWriter::new(sink);
        writer.write_packet(
            opus_head(pre_skip as u16),
            serial,
            PacketWriteEndInfo::EndPage,
            0,
        )?;
        writer.write_packet(opus_tags(), serial, PacketWriteEndInfo::EndPage, 0)?;
        Ok(Self {
            cfg,
            enc,
            writer,
            serial,
            pre_skip,
            next_seq: None,
            position: 0,
            page_packets: 0,
            held: None,
            encoded_pcm: false,
            accepted: 0,
            levels: Some(Vec::new()),
        })
    }

    pub fn push(&mut self, frame: TurnFrame<'_>) -> Result<Push, Error> {
        let (kind, duration) = if frame.payload.len() == PCM_PAYLOAD_BYTES {
            (Payload::Pcm, FRAME_48K)
        } else if let Some(d) = opus::packet_duration_48k(frame.payload) {
            (Payload::Opus, u64::from(d))
        } else {
            return Ok(Push::Invalid);
        };

        let lost = match self.next_seq {
            None => 0,
            Some(next) => match frame.sequence.wrapping_sub(next) as i16 {
                behind if behind < 0 => return Ok(Push::Duplicate),
                ahead => ahead as u64,
            },
        };
        let mut silence = lost * FRAME_48K;
        let tolerance = u64::from(self.cfg.gap_tolerance_ms) * 48;
        let ahead = (u64::from(frame.offset_ms) * 48).saturating_sub(self.position + silence);
        if ahead > tolerance {
            silence += ahead / FRAME_48K * FRAME_48K;
        }
        if self.position + silence + duration > u64::from(self.cfg.max_duration_ms) * 48 {
            return Ok(Push::Full);
        }

        for _ in 0..silence / FRAME_48K {
            self.encode_pcm(&[0; FRAME_SAMPLES])?;
            self.record_level(Some(0));
        }
        match kind {
            Payload::Pcm => {
                let mut pcm = [0i16; FRAME_SAMPLES];
                for (s, b) in pcm.iter_mut().zip(frame.payload.as_chunks::<2>().0) {
                    *s = i16::from_le_bytes(*b);
                }
                self.encode_pcm(&pcm)?;
                self.record_level(Some(frame_rms(&pcm)));
            }
            Payload::Opus => {
                self.emit(frame.payload.to_vec(), duration)?;
                self.record_level(None);
            }
        }
        self.next_seq = Some(frame.sequence.wrapping_add(1));
        self.accepted += 1;
        Ok(Push::Accepted)
    }

    pub fn accepted_frames(&self) -> u32 {
        self.accepted
    }

    /// Audio duration so far in milliseconds, silence included.
    pub fn duration_ms(&self) -> u64 {
        self.position / 48
    }

    /// Loudness bars `0..=255` over the whole turn, or `None` when the turn is empty or holds
    /// passthrough Opus packets. Values are relative to the loudest frame (but never stretched
    /// beyond [`WAVEFORM_FULL_SCALE_FLOOR`]), like the voice-note bars of a messaging app.
    pub fn waveform(&self) -> Option<[u8; WAVEFORM_BARS]> {
        waveform(self.levels.as_deref()?)
    }

    fn record_level(&mut self, level: Option<u16>) {
        match level {
            Some(level) => {
                if let Some(levels) = self.levels.as_mut() {
                    levels.push(level);
                }
            }
            None => self.levels = None,
        }
    }

    /// Flushes the encoder delay, writes the last page with the end-of-stream flag and returns
    /// the sink. `Err(Empty)` if no frame was accepted; the caller should drop the recording.
    pub fn finish(mut self) -> Result<W, Error> {
        let real_end = self.position;
        if self.encoded_pcm {
            // Enough extra frames that the decoder, after dropping pre-skip, has produced all
            // the audio that was pushed.
            while self.position < real_end + self.pre_skip {
                self.encode_pcm(&[0; FRAME_SAMPLES])?;
            }
        }
        let (packet, granule) = self.held.take().ok_or(Error::Empty)?;
        let granule = if self.encoded_pcm {
            granule.min(real_end + self.pre_skip)
        } else {
            granule
        };
        self.writer
            .write_packet(packet, self.serial, PacketWriteEndInfo::EndStream, granule)?;
        Ok(self.writer.into_inner())
    }

    fn encode_pcm(&mut self, pcm: &[i16; FRAME_SAMPLES]) -> Result<(), Error> {
        self.encoded_pcm = true;
        let packet = self.enc.encode(pcm)?;
        self.emit(packet, FRAME_48K)
    }

    fn emit(&mut self, packet: Vec<u8>, duration: u64) -> Result<(), Error> {
        if let Some((prev, granule)) = self.held.take() {
            self.page_packets += 1;
            let end = if self.page_packets >= self.cfg.packets_per_page {
                self.page_packets = 0;
                PacketWriteEndInfo::EndPage
            } else {
                PacketWriteEndInfo::NormalPacket
            };
            self.writer.write_packet(prev, self.serial, end, granule)?;
        }
        self.position += duration;
        self.held = Some((packet, self.position));
        Ok(())
    }
}

enum Payload {
    Pcm,
    Opus,
}

fn frame_rms(pcm: &[i16; FRAME_SAMPLES]) -> u16 {
    let sum: f64 = pcm.iter().map(|&s| f64::from(s).powi(2)).sum();
    (sum / FRAME_SAMPLES as f64).sqrt() as u16
}

/// Each bar is the loudest frame of its slice of the turn. A turn shorter than
/// [`WAVEFORM_BARS`] frames repeats frames across neighbouring bars.
fn waveform(levels: &[u16]) -> Option<[u8; WAVEFORM_BARS]> {
    if levels.is_empty() {
        return None;
    }
    let n = levels.len();
    let full = levels
        .iter()
        .copied()
        .max()
        .unwrap_or(0)
        .max(WAVEFORM_FULL_SCALE_FLOOR);
    let mut bars = [0u8; WAVEFORM_BARS];
    for (i, bar) in bars.iter_mut().enumerate() {
        let start = i * n / WAVEFORM_BARS;
        let end = ((i + 1) * n / WAVEFORM_BARS).max(start + 1);
        let peak = levels[start..end].iter().copied().max().unwrap_or(0);
        *bar = (u32::from(peak) * 255 / u32::from(full)) as u8;
    }
    Some(bars)
}

/// Convenience for a turn that is already complete in memory.
pub fn encode_turn<'a>(
    frames: impl IntoIterator<Item = TurnFrame<'a>>,
    serial: u32,
    cfg: Config,
) -> Result<Vec<u8>, Error> {
    let mut turn = TurnEncoder::new(Vec::new(), serial, cfg)?;
    for frame in frames {
        turn.push(frame)?;
    }
    turn.finish()
}

fn opus_head(pre_skip: u16) -> Vec<u8> {
    let mut h = Vec::with_capacity(19);
    h.extend_from_slice(b"OpusHead");
    h.push(1); // version
    h.push(1); // mono
    h.extend_from_slice(&pre_skip.to_le_bytes());
    h.extend_from_slice(&(SAMPLE_RATE as u32).to_le_bytes());
    h.extend_from_slice(&0i16.to_le_bytes()); // output gain
    h.push(0); // channel mapping family 0
    h
}

fn opus_tags() -> Vec<u8> {
    const VENDOR: &[u8] = b"kawalo-api";
    let mut t = Vec::with_capacity(8 + 4 + VENDOR.len() + 4);
    t.extend_from_slice(b"OpusTags");
    t.extend_from_slice(&(VENDOR.len() as u32).to_le_bytes());
    t.extend_from_slice(VENDOR);
    t.extend_from_slice(&0u32.to_le_bytes()); // no user comments
    t
}

#[cfg(test)]
mod tests;
