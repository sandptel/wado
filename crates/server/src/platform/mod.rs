//! The platform layer (`wado-platform`, `docs/superpowers/specs/2026-10-30-platform-layer-design.md`)
//! as this daemon uses it: the providers for this OS, and the one probed [`Platform`] every part
//! of the daemon asks for a host capability.
//!
//! Policy stays out of here — grants, Wi-Fi probation, pacing, the viewer sink's lifecycle are
//! the daemon's own, written once over the traits. This is only "which provider, and is it here".
//!
//! - [`linux`] — the Linux providers.

#[cfg(target_os = "linux")]
pub mod linux;
pub mod pty;
#[cfg(target_os = "macos")]
pub use wado_host_macos as macos;

use std::sync::OnceLock;

use wado_platform::{Platform, PlatformBuilder};

static PLATFORM: OnceLock<Platform> = OnceLock::new();

/// This OS's providers, in the order they are asked.
#[cfg(target_os = "linux")]
pub fn os_default() -> PlatformBuilder {
    PlatformBuilder::empty().with(linux::LinuxProvider::default())
}

/// This OS's providers, in the order they are asked.
#[cfg(target_os = "macos")]
pub fn os_default() -> PlatformBuilder {
    PlatformBuilder::empty()
        .with(macos::MacProvider::default())
        .with(Portable)
}

/// What every OS gets from the same code, for an OS whose provider does not offer it.
#[cfg(target_os = "macos")]
struct Portable;

#[cfg(target_os = "macos")]
impl wado_platform::Provider for Portable {
    fn id(&self) -> wado_platform::ProviderId {
        wado_platform::ProviderId("portable")
    }

    fn pty(&self) -> wado_platform::Offer<dyn wado_platform::pty::PtySpawner> {
        Some(Ok(std::sync::Arc::new(pty::NativePty)))
    }
}

/// The daemon's platform, probed on first use. The daemon probes it at start, after capturing
/// the desktop's display and Hyprland instance and before the compositor replaces them, so
/// providers that need the desktop find it.
pub fn get() -> &'static Platform {
    PLATFORM.get_or_init(|| os_default().probe())
}

/// This OS's name on the wire (`ConfigState::platform`).
pub const OS: &str = if cfg!(target_os = "macos") {
    "macos"
} else {
    "linux"
};

/// What the daemon itself offers beyond the platform capabilities: on, or why not.
const DAEMON: [&str; 4] = ["session", "sound", "notifications", "menus"];

/// Planned for this OS and not built yet, with what the client says about it. Everything not
/// named here is either on, or missing for the probe's own reason.
#[cfg(target_os = "macos")]
fn later(cap: &str) -> Option<&'static str> {
    Some(match cap {
        "session" => "A session on this Mac — its screen on your phone — is coming to macOS.",
        "sound" => "Sound from this Mac is coming to macOS.",
        "notifications" => "This Mac's notifications on your phone are coming to macOS.",
        "menus" => "App menus are coming to macOS.",
        "window_manager" => "Window and Space controls are coming to macOS.",
        _ => return None,
    })
}

#[cfg(not(target_os = "macos"))]
fn later(_cap: &str) -> Option<&'static str> {
    None
}

/// Missing on this OS for good, whatever is installed: the reason replaces the probe's.
#[cfg(target_os = "macos")]
fn never(cap: &str) -> Option<&'static str> {
    Some(match cap {
        "gamepad" => {
            "Not on macOS: a virtual game controller needs a driver Apple has to approve. The pad's keys mode works."
        }
        "viewer_sink" => {
            "Not on macOS: playing this Mac's sound on your phone needs a virtual audio driver."
        }
        "wifi" => "Not on macOS: Wi-Fi is switched in the Mac's own Control Centre.",
        "bluetooth" => "Not on macOS: Bluetooth is switched in the Mac's own Control Centre.",
        _ => return None,
    })
}

#[cfg(not(target_os = "macos"))]
fn never(_cap: &str) -> Option<&'static str> {
    None
}

/// Every host feature's state as the clients read it: the probed capabilities, then the
/// daemon's own, each on, off for a reason, or coming to this OS later.
pub fn host_caps() -> wado_protocol::caps::CapSet {
    // On a Mac a grant (Accessibility, Screen Recording) turns a capability on with no restart:
    // look again at what has been missing a few seconds. Cheap there — each probe is one call.
    #[cfg(target_os = "macos")]
    get().reprobe_stale(std::time::Duration::from_secs(5));
    mark(get().caps())
}

fn mark(mut set: wado_protocol::caps::CapSet) -> wado_protocol::caps::CapSet {
    use wado_protocol::caps::CapOff;
    for name in DAEMON {
        set.on.push(name.to_string());
    }
    // Without the desktop's own history wado keeps one, in memory (`crate::clip::ram`).
    if set.on.iter().any(|c| c == "clipboard") && !set.on.iter().any(|c| c == "clip_history") {
        set.off.retain(|o| o.cap != "clip_history");
        set.on.push("clip_history".to_string());
    }
    let (on, mut off): (Vec<_>, Vec<_>) = set
        .on
        .into_iter()
        .partition(|c| later(c).is_none() && never(c).is_none());
    let mut moved: Vec<CapOff> = off
        .drain(..)
        .map(|cap| CapOff {
            cap,
            why: String::new(),
            later: false,
        })
        .collect();
    moved.append(&mut set.off);
    for o in &mut moved {
        if let Some(why) = later(&o.cap) {
            o.why = why.to_string();
            o.later = true;
        } else if let Some(why) = never(&o.cap) {
            o.why = why.to_string();
        }
    }
    wado_protocol::caps::CapSet { on, off: moved }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wado_protocol::caps::{CapOff, CapSet};

    fn probed() -> CapSet {
        CapSet {
            on: vec!["keys".into(), "wifi".into()],
            off: vec![CapOff {
                cap: "window_manager".into(),
                why: "no Hyprland".into(),
                later: false,
            }],
        }
    }

    #[test]
    #[cfg(target_os = "macos")]
    fn macos_marks_the_session_as_coming() {
        let s = mark(probed());
        let off = |n: &str| s.off.iter().find(|o| o.cap == n).cloned().unwrap();
        assert!(off("session").later && off("session").why.contains("coming to macOS"));
        assert!(off("window_manager").later);
        assert!(!off("wifi").later && off("wifi").why.starts_with("Not on macOS"));
        assert!(s.on.contains(&"keys".to_string()));
        assert!(!s.on.contains(&"session".to_string()));
    }

    #[test]
    #[cfg(target_os = "linux")]
    fn linux_has_the_session_on() {
        let s = mark(probed());
        for n in ["session", "sound", "keys", "wifi"] {
            assert!(s.on.contains(&n.to_string()), "{n}");
        }
        assert!(s.off.iter().all(|o| !o.later));
    }
}
