//! Raw PCM from the session's audio sink: `pw-record` on its monitor, 48 kHz stereo s16.

use std::{
    io::Read,
    process::{Child, ChildStdout, Command, Stdio},
};

pub const RATE: u32 = 48_000;
pub const CHANNELS: usize = 2;

pub struct Capture {
    child: Child,
    out: ChildStdout,
}

impl Capture {
    /// `quantum_ms`: PipeWire's processing period for the capture — one Opus frame, so samples
    /// leave the graph as soon as a frame is full.
    pub fn start(sink: &str, quantum_ms: u32) -> std::io::Result<Self> {
        let props = format!(
            "{{ stream.capture.sink=true node.latency={}/48000 }}",
            RATE * quantum_ms / 1000
        );
        let mut child = Command::new("pw-record")
            .args([
                "--target",
                sink,
                "--rate",
                "48000",
                "--channels",
                "2",
                "--format",
                "s16",
            ])
            // Record what the sink *plays* (its monitor), not a microphone.
            .args(["-P", &props, "-"])
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let out = child
            .stdout
            .take()
            .ok_or_else(|| std::io::Error::other("no stdout"))?;
        Ok(Self { child, out })
    }

    /// Fill `buf` with exactly one frame's worth of samples. `false` once the stream has ended.
    pub fn read_frame(&mut self, buf: &mut [u8]) -> bool {
        self.out.read_exact(buf).is_ok()
    }
}

impl Drop for Capture {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}
