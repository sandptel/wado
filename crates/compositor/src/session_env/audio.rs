//! The session's own audio output: a PipeWire null sink its applications play into.
//!
//! Without it, an app launched in the session plays on the speakers of the computer running
//! wado — the audio equivalent of the window opening on the wrong desktop. With it, apps are
//! pointed at `wado-<instance>-<pid>` (`PULSE_SINK`, which pipewire-pulse honours, and every common
//! app speaks Pulse), and the server captures that sink's monitor for the stream
//! (`server::audio`). Nothing plays on the host.
//!
//! The node lives exactly as long as the `pw-cli` process that created it, which is what makes
//! cleanup automatic: kill the child, the sink is gone — even if the daemon itself dies.

use std::{
    io::Write,
    os::unix::process::CommandExt,
    process::{Child, Command, Stdio},
};

use tracing::{info, warn};

pub struct AudioSink {
    pub name: String,
    child: Child,
}

/// Create the sink, or `None` (logged) when PipeWire is not reachable or audio is off.
pub fn start() -> Option<AudioSink> {
    let cfg = wado_config::live::current();
    if !cfg.session.audio {
        return None;
    }
    // The pid makes it unique per daemon run: a sink left by a daemon that died would otherwise
    // share the name, and player and capture could each pick a different one.
    let name = format!("wado-{}-{}", cfg.server.instance, std::process::id());
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
         node.description=\"wado session\" media.class=Audio/Sink audio.position=[FL FR] \
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
    info!(sink = name, "session audio sink created");
    Some(AudioSink { name, child })
}

pub fn terminate(mut sink: AudioSink) {
    let _ = sink.child.kill();
    let _ = sink.child.wait();
}
