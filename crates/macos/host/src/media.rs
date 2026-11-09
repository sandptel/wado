//! What is playing on the Mac, and its controls.
//!
//! macOS keeps "now playing" behind a framework only Apple's apps may read since macOS 15.4, so
//! the card comes from the two players that answer AppleScript — Music and Spotify (each asks
//! once to be controlled, in Privacy & Security › Automation). Anything else gets the media keys,
//! which every Mac app obeys, on a card that says it has no track to show.

use std::time::Duration;

use objc2_app_kit::{NSEvent, NSEventModifierFlags, NSEventType, NSRunningApplication};
use objc2_foundation::{NSPoint, NSString};
use tokio::sync::watch;
use wado_platform::media::MediaPlayers;
use wado_platform::state::Published;
use wado_platform::{Fut, Res};
use wado_protocol::host::{MediaOp, Player};

/// The players read by AppleScript: bundle id, and the name AppleScript knows it by.
const SCRIPTABLE: [(&str, &str); 2] = [
    ("com.spotify.client", "Spotify"),
    ("com.apple.Music", "Music"),
];
/// The card for every other app.
pub const KEYS: &str = "media-keys";

/// `NX_KEYTYPE_*`, the media keys' codes.
pub const PLAY: i64 = 16;
pub const NEXT: i64 = 17;
pub const PREVIOUS: i64 = 18;

const AUTOMATION: &str =
    "Allow wado to control it in System Settings › Privacy & Security › Automation.";

pub struct MacMedia {
    state: Published<Vec<Player>>,
}

impl MacMedia {
    pub fn new() -> MacMedia {
        MacMedia {
            state: Published::new(vec![keys_card()]),
        }
    }
}

impl Default for MacMedia {
    fn default() -> Self {
        Self::new()
    }
}

fn keys_card() -> Player {
    Player {
        bus: KEYS.into(),
        app: "This Mac".into(),
        title: "Media keys".into(),
        artist: "Play, pause and skip whatever is playing".into(),
        can_next: true,
        can_prev: true,
        ..Player::default()
    }
}

fn running(bundle: &str) -> bool {
    NSRunningApplication::runningApplicationsWithBundleIdentifier(&NSString::from_str(bundle))
        .count()
        > 0
}

/// The AppleScript that reads `app`'s track as tab-separated fields.
fn read_script(app: &str) -> String {
    let art = if app == "Spotify" {
        "(artwork url of current track)"
    } else {
        "\"\""
    };
    format!(
        "tell application \"{app}\"\nif player state is stopped then return \"\"\n\
         return (name of current track) & tab & (artist of current track) & tab & (player state as string) \
         & tab & (player position as string) & tab & (duration of current track as string) & tab & {art}\nend tell"
    )
}

/// One player's card from its script's answer. Spotify's duration is in ms, Music's in seconds;
/// a locale may write `12,5` for a position.
pub fn parse(bus: &str, app: &str, out: &str) -> Option<Player> {
    let f: Vec<&str> = out.trim_end_matches('\n').split('\t').collect();
    if f.len() < 5 || f[0].is_empty() {
        return None;
    }
    let num = |s: &str| s.trim().replace(',', ".").parse::<f64>().unwrap_or(0.0);
    let length_ms = if app == "Spotify" {
        num(f[4])
    } else {
        num(f[4]) * 1000.0
    } as u64;
    Some(Player {
        bus: bus.into(),
        app: app.into(),
        title: f[0].into(),
        artist: f[1].into(),
        playing: f[2].trim() == "playing",
        position_ms: (num(f[3]) * 1000.0) as u64,
        length_ms,
        art: f
            .get(5)
            .map(|s| s.trim())
            .filter(|s| s.starts_with("https://"))
            .map(str::to_string),
        can_next: true,
        can_prev: true,
        can_seek: true,
        ..Player::default()
    })
}

async fn osascript(src: &str) -> Res<String> {
    crate::cmd::run("/usr/bin/osascript", &["-e", src], Duration::from_secs(4))
        .await
        .map_err(|e| {
            if e.contains("-1743") {
                AUTOMATION.to_string()
            } else {
                e
            }
        })
}

async fn read() -> Vec<Player> {
    let mut out = Vec::new();
    for (bundle, app) in SCRIPTABLE {
        // Asked only of a player that is running: a `tell` would otherwise launch it.
        if running(bundle)
            && let Ok(s) = osascript(&read_script(app)).await
            && let Some(p) = parse(bundle, app, &s)
        {
            out.push(p);
        }
    }
    out.sort_by_key(|p| !p.playing);
    out.push(keys_card());
    out
}

/// The `NSEvent` fields of a media key press: its modifier flags and `data1`.
pub fn key_event(key: i64, down: bool) -> (usize, isize) {
    let state = if down { 0xa } else { 0xb };
    ((state << 8) as usize, ((key << 16) | (state << 8)) as isize)
}

/// Press and release a media key, as the keyboard's own does.
pub fn press(key: i64) -> Res<()> {
    crate::tcc::need_accessibility()?;
    for down in [true, false] {
        let (flags, data1) = key_event(key, down);
        let ev = NSEvent::otherEventWithType_location_modifierFlags_timestamp_windowNumber_context_subtype_data1_data2(
            NSEventType::SystemDefined,
            NSPoint::new(0.0, 0.0),
            NSEventModifierFlags(flags),
            0.0,
            0,
            None,
            8,
            data1,
            -1,
        )
        .ok_or("macOS would not make the media key")?;
        let cg = ev.CGEvent().ok_or("macOS would not post the media key")?;
        // SAFETY: `cg` is a live event, held until after the post.
        unsafe {
            crate::input::cg::post_event(
                (&*cg as *const objc2_core_graphics::CGEvent)
                    .cast_mut()
                    .cast(),
            )
        };
    }
    Ok(())
}

fn script_op(op: &MediaOp, app: &str) -> Option<String> {
    let verb = match op {
        MediaOp::PlayPause => "playpause".to_string(),
        MediaOp::Next => "next track".to_string(),
        MediaOp::Previous => "previous track".to_string(),
        MediaOp::SeekTo { ms } => format!("set player position to {}", *ms as f64 / 1000.0),
        MediaOp::Shuffle { on } if app == "Spotify" => format!("set shuffling to {on}"),
        MediaOp::Shuffle { on } => format!("set shuffle enabled to {on}"),
        MediaOp::Loop { .. } => return None,
    };
    Some(format!("tell application \"{app}\" to {verb}"))
}

impl MediaPlayers for MacMedia {
    fn state(&self) -> watch::Receiver<Vec<Player>> {
        self.state.subscribe()
    }

    fn fresh(&self, within: Duration) -> Fut<'_, ()> {
        Box::pin(self.state.fresh(within, read))
    }

    fn act(&self, bus: String, op: MediaOp) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            if let Some((_, app)) = SCRIPTABLE.iter().find(|(b, _)| *b == bus) {
                let src = script_op(&op, app)
                    .ok_or_else(|| format!("{app} does not take that from wado on macOS"))?;
                osascript(&src).await?;
            } else {
                match op {
                    MediaOp::PlayPause => press(PLAY)?,
                    MediaOp::Next => press(NEXT)?,
                    MediaOp::Previous => press(PREVIOUS)?,
                    _ => {
                        return Err(
                            "Controls only on macOS for this app: play, pause and skip.".into()
                        );
                    }
                }
            }
            self.state.publish(read().await);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn media_key_fields() {
        assert_eq!(key_event(PLAY, true), (0xa00, (16 << 16 | 0xa00) as isize));
        assert_eq!(key_event(NEXT, false), (0xb00, (17 << 16 | 0xb00) as isize));
    }

    #[test]
    fn applescript_parse_spotify() {
        let p = parse(
            "com.spotify.client",
            "Spotify",
            "Song\tBand\tplaying\t12,5\t200000\thttps://i.scdn.co/x\n",
        )
        .unwrap();
        assert_eq!(
            (p.title.as_str(), p.artist.as_str(), p.playing),
            ("Song", "Band", true)
        );
        assert_eq!((p.position_ms, p.length_ms), (12_500, 200_000));
        assert_eq!(p.art.as_deref(), Some("https://i.scdn.co/x"));
    }

    #[test]
    fn applescript_parse_music_seconds() {
        let p = parse(
            "com.apple.Music",
            "Music",
            "Song\tBand\tpaused\t3.0\t180.5\t\n",
        )
        .unwrap();
        assert_eq!((p.playing, p.length_ms, p.art), (false, 180_500, None));
    }

    #[test]
    fn stopped_is_no_card() {
        assert!(parse("com.apple.Music", "Music", "\n").is_none());
    }

    #[test]
    fn loop_is_not_scripted() {
        assert!(
            script_op(
                &MediaOp::Loop {
                    mode: "Track".into()
                },
                "Music"
            )
            .is_none()
        );
        assert_eq!(
            script_op(&MediaOp::Next, "Spotify").unwrap(),
            "tell application \"Spotify\" to next track"
        );
    }
}
