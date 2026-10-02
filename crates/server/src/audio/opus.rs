//! PCM → Opus, through libopus via the ffmpeg binding: 10 ms frames, low-delay mode.

use ff::format::{Sample, sample::Type};
use ffmpeg_the_third as ff;

use super::capture::{CHANNELS, RATE};

/// Samples per channel in one frame: 10 ms. Half Opus's default 20 ms, because audio that
/// arrives late against the picture is the thing people notice.
pub const FRAME: usize = (RATE / 100) as usize;

pub struct Opus {
    enc: ff::encoder::Audio,
    pts: i64,
}

impl Opus {
    pub fn new(kbps: u32) -> Result<Self, ff::Error> {
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
        opts.set("frame_duration", "10");
        Ok(Self {
            enc: enc.open_as_with(codec, opts)?,
            pts: 0,
        })
    }

    /// Encode one frame of interleaved s16 PCM (`FRAME * CHANNELS * 2` bytes), handing each
    /// packet that comes out to `out`.
    pub fn encode(&mut self, pcm: &[u8], mut out: impl FnMut(&[u8])) -> Result<(), ff::Error> {
        let mut frame = ff::frame::Audio::new(
            Sample::I16(Type::Packed),
            FRAME,
            ff::ChannelLayoutMask::STEREO,
        );
        frame.set_rate(RATE);
        frame.data_mut(0)[..pcm.len()].copy_from_slice(pcm);
        frame.set_pts(Some(self.pts));
        self.pts += FRAME as i64;
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

/// Bytes of PCM in one frame.
pub const FRAME_BYTES: usize = FRAME * CHANNELS * 2;
