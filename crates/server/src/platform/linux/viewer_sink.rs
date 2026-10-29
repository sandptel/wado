//! "This phone" ([`ViewerSink`] on Linux): a PipeWire null sink whose sound goes to the viewer
//! instead of a speaker.
//!
//! It exists only while a device is connected, and is described after that device ("wado ·
//! Android · Chrome"), so the computer's mixer shows who it plays to — not a "wado viewer" per
//! daemon left standing with nobody connected (reported 2026-10-04). When it is made and remade
//! is the daemon's policy (`crate::host::viewer`). Session apps are pointed at it (`PULSE_SINK`,
//! via `CompositorCommand::AudioSink`), so a session plays on the phone by default; host apps
//! play where they always did until moved here from the Sound page. `crate::audio` captures its
//! monitor while a viewer is listening.
//!
//! The node lives exactly as long as the `pw-cli` process that created it, which is what makes
//! cleanup automatic: kill the child, the sink is gone — even if the daemon itself dies.

use std::{
    io::Write,
    os::unix::process::CommandExt,
    process::{Child, Command, Stdio},
};

use tracing::{info, warn};
use wado_platform::Res;
use wado_platform::audio::{SinkHandle, ViewerSink};

/// Shown when session audio is turned off in the config.
pub const AUDIO_OFF: &str = "session audio is off (session { audio #false })";

/// The null sink under a name fixed for this daemon run.
pub struct NullSink {
    name: String,
}

impl NullSink {
    /// The viewer sink, or why there is none (audio turned off).
    pub fn new() -> Result<NullSink, String> {
        name()
            .map(|name| NullSink { name })
            .ok_or_else(|| AUDIO_OFF.to_string())
    }
}

impl ViewerSink for NullSink {
    fn name(&self) -> String {
        self.name.clone()
    }

    fn open(&self, description: String) -> Res<Box<dyn SinkHandle>> {
        start(&self.name, &description)
            .map(|s| Box::new(s) as Box<dyn SinkHandle>)
            .ok_or_else(|| "the viewer's sound output could not be made".to_string())
    }
}

/// A live sink: the `pw-cli` process holding it. Dropping it removes the sink.
pub struct AudioSink {
    pub name: String,
    child: Child,
}

impl SinkHandle for AudioSink {}

impl Drop for AudioSink {
    fn drop(&mut self) {
        let _ = self.child.kill();
        let _ = self.child.wait();
    }
}

/// The sink's node name for this daemon run, or `None` when audio is off. Fixed for the run, so
/// apps pointed at it (`PULSE_SINK`) and the capture keep one target while the sink itself comes
/// and goes. The pid makes it unique: a sink left by a daemon that died never shares it.
pub fn name() -> Option<String> {
    let cfg = wado_config::live::current();
    cfg.session
        .audio
        .then(|| format!("wado-{}-{}", cfg.server.instance, std::process::id()))
}

/// Create the sink as `description`, or `None` (logged) when PipeWire is not reachable.
pub fn start(name: &str, description: &str) -> Option<AudioSink> {
    let name = name.to_string();
    let description = description.replace(['"', '\\', '\n'], " ");
    let mut cmd = Command::new("pw-cli");
    cmd.stdin(Stdio::piped())
        .stdout(Stdio::null())
        .stderr(Stdio::null());
    // If the daemon dies — even by a crash that runs no cleanup — the kernel ends pw-cli, and
    // with it the sink. Measured: without this, every killed test daemon left a sink behind.
    // SAFETY: prctl is async-signal-safe and touches only this child.
    unsafe {
        cmd.pre_exec(|| {
            libc::prctl(libc::PR_SET_PDEATHSIG, libc::SIGTERM);
            Ok(())
        });
    }
    let mut child = match cmd.spawn() {
        Ok(c) => c,
        Err(e) => {
            warn!("pw-cli did not start ({e}) — no session audio; apps play on this computer");
            return None;
        }
    };
    let create = format!(
        "create-node adapter {{ factory.name=support.null-audio-sink node.name={name} \
         node.description=\"{description}\" media.class=Audio/Sink audio.position=[FL FR] \
         object.linger=false }}\n"
    );
    // stdin is kept open: closing it would end pw-cli, and with it the node.
    let ok = child
        .stdin
        .as_mut()
        .is_some_and(|s| s.write_all(create.as_bytes()).is_ok());
    if !ok {
        warn!("could not create the session's audio sink — apps play on this computer");
        let _ = child.kill();
        return None;
    }
    info!(sink = name, description, "viewer audio sink created");
    Some(AudioSink { name, child })
}
