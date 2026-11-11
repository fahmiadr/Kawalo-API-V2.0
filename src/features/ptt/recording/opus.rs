//! Safe wrappers over `unsafe-libopus` (libopus 1.3.1 transpiled to Rust, so no C toolchain or
//! system library is involved). The raw API is `unsafe` and its `ctl` calls panic on a wrongly
//! typed argument, so every call is confined to this file.

use std::ptr::NonNull;

use unsafe_libopus::{
    OPUS_APPLICATION_VOIP, OPUS_GET_LOOKAHEAD_REQUEST, OPUS_OK, OPUS_SET_BITRATE_REQUEST,
    OPUS_SET_COMPLEXITY_REQUEST, OPUS_SET_DTX_REQUEST, OPUS_SET_INBAND_FEC_REQUEST,
    OPUS_SET_SIGNAL_REQUEST, OPUS_SET_VBR_REQUEST, OPUS_SIGNAL_VOICE, OpusEncoder, opus_encode,
    opus_encoder_create, opus_encoder_ctl, opus_encoder_destroy, opus_packet_get_nb_samples,
    opus_strerror,
};

pub const SAMPLE_RATE: i32 = 16_000;
/// 20 ms at 16 kHz.
pub const FRAME_SAMPLES: usize = 320;
/// Worst case for one 20 ms packet at any sane bitrate; libopus rejects a smaller output buffer
/// instead of truncating.
const MAX_PACKET_BYTES: usize = 1275;

#[derive(Debug, thiserror::Error)]
#[error("opus: {0}")]
pub struct OpusError(String);

fn check(code: i32) -> Result<i32, OpusError> {
    if code < OPUS_OK {
        Err(OpusError(opus_strerror(code).to_owned()))
    } else {
        Ok(code)
    }
}

#[derive(Debug, Clone, Copy)]
pub struct EncoderSettings {
    pub bitrate_bps: i32,
    /// 0..=10; libopus default is 9 at 16 kHz.
    pub complexity: i32,
    pub vbr: bool,
    pub dtx: bool,
}

pub struct Encoder {
    raw: NonNull<OpusEncoder>,
}

// The encoder owns its state exclusively and has no thread affinity.
unsafe impl Send for Encoder {}

impl Encoder {
    pub fn new(settings: EncoderSettings) -> Result<Self, OpusError> {
        let mut err = OPUS_OK;
        // SAFETY: `err` outlives the call; arguments are valid constants.
        let ptr = unsafe { opus_encoder_create(SAMPLE_RATE, 1, OPUS_APPLICATION_VOIP, &mut err) };
        check(err)?;
        let raw = NonNull::new(ptr).ok_or_else(|| OpusError("allocation failed".into()))?;
        let mut enc = Self { raw };
        enc.ctl(OPUS_SET_BITRATE_REQUEST, settings.bitrate_bps)?;
        enc.ctl(OPUS_SET_COMPLEXITY_REQUEST, settings.complexity)?;
        enc.ctl(OPUS_SET_VBR_REQUEST, i32::from(settings.vbr))?;
        enc.ctl(OPUS_SET_DTX_REQUEST, i32::from(settings.dtx))?;
        enc.ctl(OPUS_SET_SIGNAL_REQUEST, OPUS_SIGNAL_VOICE)?;
        enc.ctl(OPUS_SET_INBAND_FEC_REQUEST, 0)?;
        Ok(enc)
    }

    fn ctl(&mut self, request: i32, value: i32) -> Result<(), OpusError> {
        // SAFETY: `raw` is a live encoder; every request used here takes a single i32.
        check(unsafe { opus_encoder_ctl!(self.raw.as_ptr(), request, value) }).map(drop)
    }

    /// Encoder delay in 16 kHz samples.
    pub fn lookahead(&mut self) -> Result<i32, OpusError> {
        let mut out = 0i32;
        // SAFETY: `raw` is live and `out` outlives the call.
        check(unsafe {
            opus_encoder_ctl!(self.raw.as_ptr(), OPUS_GET_LOOKAHEAD_REQUEST, &mut out)
        })?;
        Ok(out)
    }

    pub fn encode(&mut self, pcm: &[i16; FRAME_SAMPLES]) -> Result<Vec<u8>, OpusError> {
        let mut out = vec![0u8; MAX_PACKET_BYTES];
        // SAFETY: `pcm` holds exactly FRAME_SAMPLES samples and `out` MAX_PACKET_BYTES bytes,
        // the sizes passed alongside the pointers.
        let n = check(unsafe {
            opus_encode(
                self.raw.as_ptr(),
                pcm.as_ptr(),
                FRAME_SAMPLES as i32,
                out.as_mut_ptr(),
                MAX_PACKET_BYTES as i32,
            )
        })?;
        out.truncate(n as usize);
        Ok(out)
    }
}

impl Drop for Encoder {
    fn drop(&mut self) {
        // SAFETY: `raw` came from `opus_encoder_create` and is destroyed exactly once.
        unsafe { opus_encoder_destroy(self.raw.as_ptr()) }
    }
}

/// Duration of a foreign Opus packet in 48 kHz samples, or `None` if libopus cannot parse it.
pub fn packet_duration_48k(packet: &[u8]) -> Option<u32> {
    let len = i32::try_from(packet.len()).ok()?;
    // SAFETY: pointer and length describe `packet`.
    let n = unsafe { opus_packet_get_nb_samples(packet.as_ptr(), len, 48_000) };
    u32::try_from(n).ok().filter(|&n| n > 0)
}

#[cfg(test)]
pub use decoder::Decoder;

#[cfg(test)]
mod decoder {
    use super::{OpusError, check};
    use std::ptr::NonNull;
    use unsafe_libopus::{
        OPUS_OK, OpusDecoder, opus_decode, opus_decoder_create, opus_decoder_destroy,
    };

    pub struct Decoder(NonNull<OpusDecoder>);

    impl Decoder {
        pub fn new(rate: i32) -> Result<Self, OpusError> {
            let mut err = OPUS_OK;
            // SAFETY: `err` outlives the call.
            let ptr = unsafe { opus_decoder_create(rate, 1, &mut err) };
            check(err)?;
            NonNull::new(ptr)
                .map(Self)
                .ok_or_else(|| OpusError("allocation failed".into()))
        }

        pub fn decode(&mut self, packet: &[u8], out: &mut Vec<i16>) -> Result<(), OpusError> {
            // 120 ms at 48 kHz is the largest packet.
            let mut buf = vec![0i16; 5760];
            // SAFETY: pointers and lengths describe `packet` and `buf`.
            let n = check(unsafe {
                opus_decode(
                    self.0.as_ptr(),
                    packet.as_ptr(),
                    packet.len() as i32,
                    buf.as_mut_ptr(),
                    buf.len() as i32,
                    0,
                )
            })?;
            out.extend_from_slice(&buf[..n as usize]);
            Ok(())
        }
    }

    impl Drop for Decoder {
        fn drop(&mut self) {
            // SAFETY: created by `opus_decoder_create`, destroyed once.
            unsafe { opus_decoder_destroy(self.0.as_ptr()) }
        }
    }
}
