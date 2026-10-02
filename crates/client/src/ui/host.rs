//! Talking to the computer itself (`server::host`): small helpers the Sound and Network pages,
//! the sound card and the tiles share.

use wado_protocol::{host::Sink, HostAction, HostState};

use crate::bridge;

pub fn act(action: HostAction) {
    bridge::call(format!("window.__wado.hostDo({});", bridge::js(&action)));
}

/// Sound is going to the phone: the default output is the phone sink.
pub fn on_phone(h: &HostState) -> bool {
    h.audio
        .phone_sink
        .as_deref()
        .is_some_and(|p| p == h.audio.default_sink)
}

/// The computer's own speakers: the default output when that is not the phone, else the first
/// real one — what "Computer" means in the Plays-on switch and the volume slider.
pub fn speaker(h: &HostState) -> Option<&Sink> {
    let real = |s: &&Sink| Some(s.name.as_str()) != h.audio.phone_sink.as_deref();
    h.audio
        .sinks
        .iter()
        .filter(real)
        .find(|s| s.name == h.audio.default_sink)
        .or_else(|| h.audio.sinks.iter().find(real))
}

/// Everything to the phone, and start listening if no session is carrying the sound.
pub fn play_on_phone(h: &HostState) {
    if let Some(p) = h.audio.phone_sink.clone() {
        act(HostAction::AllTo { sink: p });
        bridge::call("window.__wado.listenStart(); window.__wado.audioUnlock();".to_string());
    }
}

pub fn play_on_computer(h: &HostState) {
    if let Some(s) = speaker(h) {
        act(HostAction::AllTo {
            sink: s.name.clone(),
        });
        bridge::call("window.__wado.listenStop();".to_string());
    }
}
