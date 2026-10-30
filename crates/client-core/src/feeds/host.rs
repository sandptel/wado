//! A computer's `HostState`, as its card and (from P2) its page show it. Honest status: what the
//! computer did not report is `None`, never a zero that reads as a real value.

use wado_protocol::host::HostState;

use crate::os_logo;

#[derive(Debug, Clone, PartialEq)]
pub struct NowPlaying {
    pub app: String,
    pub title: String,
    pub artist: String,
    pub playing: bool,
}

#[derive(Debug, Clone, PartialEq)]
pub struct HostView {
    /// The hostname: what a card is named after until the person names it.
    pub name: String,
    /// os-release `PRETTY_NAME`.
    pub os: String,
    pub os_id: String,
    pub os_like: Vec<String>,
    /// The OS logo's slug ([`os_logo::slug`]).
    pub logo: &'static str,
    pub desktop: String,
    pub uptime_s: u64,
    /// What the computer can be asked to do here: `wifi`, `bluetooth`, `brightness`, `media`.
    pub caps: Vec<String>,
    pub playing: Option<NowPlaying>,
    /// The default output's volume, 0..1 (above 1 is boost).
    pub volume: Option<f32>,
    pub brightness: Option<f32>,
    pub awake: Option<bool>,
}

/// The view of one `HostState`.
pub fn view(s: &HostState) -> HostView {
    let i = &s.info;
    let mut caps = Vec::new();
    if s.wifi.is_some() {
        caps.push("wifi".to_string());
    }
    if s.bluetooth.is_some() {
        caps.push("bluetooth".to_string());
    }
    if s.brightness.is_some() {
        caps.push("brightness".to_string());
    }
    if !s.media.is_empty() {
        caps.push("media".to_string());
    }
    let volume = s
        .audio
        .sinks
        .iter()
        .find(|k| k.name == s.audio.default_sink)
        .map(|k| k.volume);
    let playing = s.media.first().map(|p| NowPlaying {
        app: p.app.clone(),
        title: p.title.clone(),
        artist: p.artist.clone(),
        playing: p.playing,
    });
    HostView {
        name: i.hostname.clone(),
        os: i.os.clone(),
        os_id: i.os_id.clone(),
        os_like: i.os_like.clone(),
        logo: os_logo::slug(&i.os_id, &i.os_like),
        desktop: i.desktop.clone(),
        uptime_s: i.uptime_s,
        caps,
        playing,
        volume,
        brightness: s.brightness,
        awake: Some(s.awake),
    }
}

/// How often to ask for the host state: every 5 s while it is on screen, never off screen.
pub fn poll_ms(visible: bool) -> Option<u64> {
    visible.then_some(5000)
}

#[cfg(test)]
mod tests {
    use super::*;
    use wado_protocol::host::{HostAudio, Player, Sink, SysInfo};

    #[test]
    fn host_state_maps_os_and_logo() {
        let s = HostState {
            info: SysInfo {
                hostname: "linux-box".into(),
                os: "CachyOS".into(),
                os_id: "cachyos".into(),
                os_like: vec!["arch".into()],
                desktop: "Hyprland".into(),
                uptime_s: 99,
                ..Default::default()
            },
            audio: HostAudio {
                sinks: vec![Sink {
                    id: 1,
                    name: "out".into(),
                    label: "Speakers".into(),
                    volume: 0.4,
                    muted: false,
                }],
                default_sink: "out".into(),
                ..Default::default()
            },
            media: vec![Player {
                app: "Spotify".into(),
                title: "Song".into(),
                playing: true,
                ..Default::default()
            }],
            brightness: Some(0.7),
            awake: true,
            ..Default::default()
        };
        let v = view(&s);
        assert_eq!(v.name, "linux-box");
        assert_eq!(v.os, "CachyOS");
        assert_eq!(v.logo, "archlinux");
        assert_eq!(v.desktop, "Hyprland");
        assert_eq!(v.volume, Some(0.4));
        assert_eq!(v.brightness, Some(0.7));
        assert_eq!(v.awake, Some(true));
        assert_eq!(v.playing.unwrap().title, "Song");
        assert_eq!(v.caps, ["brightness", "media"]);
    }

    #[test]
    fn missing_fields_are_none_not_zero() {
        let v = view(&HostState::default());
        assert_eq!(v.volume, None, "no default sink: no volume, not 0");
        assert_eq!(v.brightness, None);
        assert!(v.playing.is_none());
        assert!(v.caps.is_empty());
        assert_eq!(v.logo, "linux");
        assert_eq!(poll_ms(true), Some(5000));
        assert_eq!(poll_ms(false), None);
    }
}
