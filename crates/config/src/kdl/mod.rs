//! KDL text → [`Config`].
//!
//! Two steps, so each stays small: [`tree`] maps the KDL document (following `include`s) onto a
//! JSON value and remembers where each key came from; serde then decodes that value into the
//! schema. A decode error's path is looked up in [`tree::Spans`] to put a line:col on it.

pub mod merge;
pub mod tree;

use std::path::{Path, PathBuf};

use crate::{Config, ConfigError};

pub fn load(path: &Path) -> Result<Config, ConfigError> {
    load_tracked(path).map(|(c, _)| c)
}

/// Load, and say which files were read — what a watcher has to watch.
///
/// `ui.kdl` (the file wado writes for the client) is read after the main file unless the main
/// file `include`s it itself, in which case its position there decides what wins. So client
/// edits work out of the box, and someone who wants their file to win can say so.
pub fn load_tracked(path: &Path) -> Result<(Config, Vec<PathBuf>), ConfigError> {
    let mut spans = tree::Spans::default();
    let mut value = tree::read(path, &mut spans)?;
    let ui = path.with_file_name(crate::paths::UI_FILE);
    if !spans.files().contains(&ui) && ui.exists() {
        let extra = tree::read(&ui, &mut spans)?;
        merge::deep(&mut value, extra);
    }
    let files = spans.files();
    Ok((decode(value, &spans)?, files))
}

/// Parse KDL text with no file behind it (tests, `wado msg`, the client's preview).
pub fn parse(src: &str) -> Result<Config, ConfigError> {
    let mut spans = tree::Spans::default();
    let value = tree::read_str(Path::new("<input>"), src, &mut spans)?;
    decode(value, &spans)
}

fn decode(value: serde_json::Value, spans: &tree::Spans) -> Result<Config, ConfigError> {
    // Gestures merge over the defaults rather than replacing them: a file that rebinds one
    // swipe keeps the other three.
    let mut value = value;
    if let Some(g) = value.get_mut("gestures") {
        let mut all = serde_json::to_value(crate::schema::gestures::defaults()).unwrap_or_default();
        merge::deep(&mut all, g.take());
        *g = all;
    }
    let cfg: Config = serde_path_to_error::deserialize(value).map_err(|e| {
        let path = e.path().to_string();
        let msg = e.into_inner().to_string();
        spans.error(&path, format!("{path}: {msg}"))
    })?;
    cfg.check()
        .map_err(|(path, msg)| spans.error(&path, format!("{path}: {msg}")))?;
    Ok(cfg)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn default_kdl_parses_to_defaults() {
        // The shipped starting config is all comments and defaults: it must parse, and it must
        // mean exactly the built-in defaults, or first run changes behaviour.
        assert_eq!(parse(crate::DEFAULT_KDL).unwrap(), Config::default());
    }

    #[test]
    fn sections_decode() {
        let c = parse(
            r#"
            server {
                relay "wss://r.example"
                udp-slice 2
                turn {
                    url "turn:a:3478" "turns:b:5349"
                    user "u"
                    pass "p"
                }
            }
            stream { encoder "software"; max-fps 90 }
            session { env { GDK_SCALE "2" }; autostart "foot"; autostart "firefox" }
            shells { enabled #false }
            "#,
        )
        .unwrap();
        assert_eq!(c.server.relay.as_deref(), Some("wss://r.example"));
        assert_eq!(c.server.udp_slice, 2);
        assert_eq!(c.server.turn.unwrap().url.len(), 2);
        assert_eq!(c.stream.max_fps, Some(90));
        assert_eq!(c.session.env["GDK_SCALE"], "2");
        assert_eq!(c.session.autostart, ["foot", "firefox"]);
        assert!(!c.shells.enabled);
    }

    #[test]
    fn a_typo_is_an_error_with_a_position() {
        let e = parse("stream {\n    max-fsp 90\n}").unwrap_err();
        assert_eq!(e.line, 2, "{e}");
        assert!(e.message.contains("max-fsp"), "{e}");
    }

    #[test]
    fn a_wrong_type_is_an_error_with_a_position() {
        let e = parse("stream {\n  max-fps \"lots\"\n}").unwrap_err();
        assert_eq!(e.line, 2, "{e}");
    }

    #[test]
    fn binds_and_gestures_are_checked_with_positions() {
        let c = parse("binds {\n  mod \"ctrl+alt\"\n  Mod+Q \"close-window\"\n}\ngestures { swipe-3-up \"keyboard\"; }").unwrap();
        assert_eq!(c.binds.parsed().unwrap().len(), 1);
        assert_eq!(c.gestures["swipe-3-up"], "keyboard");
        assert_eq!(
            c.gestures["swipe-3-left"], "back",
            "the other defaults survive"
        );
        let e = parse("binds {\n  Mod+Q \"explode\"\n}").unwrap_err();
        assert_eq!(e.line, 2, "{e}");
        assert!(parse("gestures { swipe-4-up \"back\"; }").is_err());
    }

    #[test]
    fn a_syntax_error_has_a_position() {
        let e = parse("server {\n  listen \"x\n").unwrap_err();
        assert!(e.line >= 2, "{e}");
    }
}
