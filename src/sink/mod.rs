pub mod file;
pub mod udp;

/// Consumes encoded H.264 Annex-B NAL data for one frame.
pub trait FrameSink: Send {
    fn send(&mut self, nal_data: &[u8]);
}
