//! PCM → Opus, through libopus via the ffmpeg binding: 10 ms frames, low-delay mode.

use ff::format::{Sample, sample::Type};
use ffmpeg_the_third as ff;

use super::capture::{CHANNELS, RATE};

/// Samples per channel in a frame of `ms` milliseconds (10 by default, 5 for "Low-latency
/// audio" — both well under Opus's default 20, because audio late against the picture is the
/// thing people notice, and with A/V sync video waits for it).
pub fn frame(ms: u32) -> usize {
    (RATE * ms / 1000) as usize
}

/// Bytes of interleaved s16 PCM in one frame.
pub fn frame_bytes(ms: u32) -> usize {
    frame(ms) * CHANNELS * 2
}

pub struct Opus {
    enc: ff::encoder::Audio,
    pts: i64,
    frame: usize,
}

impl Opus {
    pub fn new(kbps: u32, frame_ms: u32) -> Result<Self, ff::Error> {
        ff::init()?;
        let codec = ff::encoder::find_by_name("libopus").ok_or(ff::Error::EncoderNotFound)?;
        let mut enc = ff::codec::Context::new_with_codec(codec)
            .encoder()
            .audio()?;
        enc.set_rate(RATE as i32);
        enc.set_ch_layout(ff::ChannelLayout::STEREO);
        enc.set_format(Sample::I16(Type::Packed));
        enc.set_bit_rate(kbps as usize * 1000);
        enc.set_time_base((1, RATE as i32));
        let mut opts = ff::Dictionary::new();
        opts.set("application", "lowdelay");
        opts.set("frame_duration", &frame_ms.to_string());
        Ok(Self {
            enc: enc.open_as_with(codec, opts)?,
            pts: 0,
            frame: frame(frame_ms),
        })
    }

    /// Encode one frame of interleaved s16 PCM (`frame_bytes(ms)` bytes), handing each
    /// packet that comes out to `out`.
    pub fn encode(&mut self, pcm: &[u8], mut out: impl FnMut(&[u8])) -> Result<(), ff::Error> {
        let mut frame = ff::frame::Audio::new(
            Sample::I16(Type::Packed),
            self.frame,
            ff::ChannelLayoutMask::STEREO,
        );
        frame.set_rate(RATE);
        frame.data_mut(0)[..pcm.len()].copy_from_slice(pcm);
        frame.set_pts(Some(self.pts));
        self.pts += self.frame as i64;
        self.enc.send_frame(&frame)?;
        let mut pkt = ff::Packet::empty();
        while self.enc.receive_packet(&mut pkt).is_ok() {
            if let Some(d) = pkt.data() {
                out(d);
            }
        }
        Ok(())
    }
}
