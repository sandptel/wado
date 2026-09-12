pub mod channel;
pub mod file;

/// Consumes encoded H.264 Annex-B NAL data for one frame.
pub trait FrameSink: Send {
    fn send(&mut self, nal_data: &[u8]);

    /// Frames this sink could not accept, cumulative for the session.
    ///
    /// The render loop reads it as a *congestion signal* — the only one available without a
    /// network round trip, because the sink is the boundary where "the link cannot keep up"
    /// first becomes observable. Monotonic, so callers must take a delta; see
    /// `crate::congestion`. Defaults to zero for sinks that cannot drop (a file never refuses).
    fn dropped(&self) -> u64 {
        0
    }
}
