//! Leave out what the viewer already has: a player's cover art and app icon are `data:` URIs of
//! up to 128 KiB, and the state is re-sent every second or two while the control centre is
//! open — most of a megabit a second on the same downlink as the video, for pictures that had
//! not changed. Each is sent once per viewer; afterwards a repeat goes as `art_same` /
//! `icon_same` and the client keeps the copy it has.

use std::collections::HashMap;
use std::hash::{DefaultHasher, Hash, Hasher};
use std::sync::Mutex;

use wado_protocol::HostState;

/// What this viewer was last sent, per player: hashes of (art, icon).
#[derive(Default)]
pub struct Sent(Mutex<HashMap<String, (u64, u64)>>);

impl Sent {
    /// A new viewer has nothing yet.
    pub fn clear(&self) {
        self.0.lock().unwrap_or_else(|e| e.into_inner()).clear();
    }

    pub fn trim(&self, state: &mut HostState) {
        let mut sent = self.0.lock().unwrap_or_else(|e| e.into_inner());
        for p in &mut state.media {
            let now = (hash(&p.art), hash(&p.icon));
            let before = sent.insert(p.bus.clone(), now);
            if let Some((art, icon)) = before {
                if art == now.0 && p.art.is_some() {
                    p.art = None;
                    p.art_same = true;
                }
                if icon == now.1 && p.icon.is_some() {
                    p.icon = None;
                    p.icon_same = true;
                }
            }
        }
    }
}

fn hash(v: &Option<String>) -> u64 {
    let mut h = DefaultHasher::new();
    v.hash(&mut h);
    h.finish()
}

#[cfg(test)]
mod tests {
    use super::Sent;
    use wado_protocol::{HostState, host::Player};

    #[test]
    fn a_picture_goes_once_then_as_same() {
        let player = Player {
            bus: "vlc".into(),
            art: Some("data:x".into()),
            icon: Some("data:i".into()),
            ..Default::default()
        };
        let state = HostState {
            media: vec![player],
            ..Default::default()
        };
        let sent = Sent::default();
        let mut a = state.clone();
        sent.trim(&mut a);
        assert!(a.media[0].art.is_some() && !a.media[0].art_same);
        let mut b = state.clone();
        sent.trim(&mut b);
        assert!(b.media[0].art.is_none() && b.media[0].art_same && b.media[0].icon_same);
        sent.clear();
        let mut c = state;
        sent.trim(&mut c);
        assert!(c.media[0].art.is_some(), "a new viewer gets it again");
    }
}
