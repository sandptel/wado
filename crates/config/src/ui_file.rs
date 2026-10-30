//! Writing `ui.kdl` — the client's edits, kept out of the hand-written `config.kdl`.
//!
//! Every write is checked by loading the whole config afterwards; a write that would make it
//! invalid is undone and returned as the error, so the client cannot leave the daemon on a
//! broken file.

use std::path::{Path, PathBuf};

use kdl::{KdlDocument, KdlEntry, KdlNode, KdlValue};

use crate::{ConfigError, paths, schema::KEYED_NODES};

const HEADER: &str = "// Written by wado from the client. Values here override config.kdl unless config.kdl\n\
// includes this file itself. Edit config.kdl instead; this file is rewritten freely.\n";

/// Set `key` (`stream.max-fps`) to `value`, or remove it with `None`.
pub fn set(key: &str, value: Option<KdlValue>) -> Result<(), ConfigError> {
    set_values(key, value.map(|v| vec![v]))
}

/// Set `key` to a node with these arguments (`quick "a" "b"`; none is a bare `quick`), or
/// remove it with `None`. A keyed section names its block in the path: `device.<id>.trackpad`.
pub fn set_values(key: &str, values: Option<Vec<KdlValue>>) -> Result<(), ConfigError> {
    set_at(&paths::config_file(), &paths::ui_file(), key, values)
}

/// Save a device's preference blob under `device "<id>" { name …; prefs … }`.
pub fn set_device(id: &str, name: &str, prefs: &str) -> Result<(), ConfigError> {
    edit(&paths::config_file(), &paths::ui_file(), |doc| {
        let body = keyed(doc, "device", id).ensure_children();
        put(body, "name", Some(vec![KdlValue::String(name.into())]));
        put(body, "prefs", Some(vec![KdlValue::String(prefs.into())]));
    })
}

/// A value typed at a prompt or sent by the client, as KDL: `true`, `90`, `1.5`, else a string.
/// Empty means unset.
pub fn parse_value(s: &str) -> Option<KdlValue> {
    let s = s.trim();
    Some(match s {
        "" | "null" => return None,
        "true" | "#true" => KdlValue::Bool(true),
        "false" | "#false" => KdlValue::Bool(false),
        _ => s
            .parse::<i128>()
            .map(KdlValue::Integer)
            .or_else(|_| s.parse::<f64>().map(KdlValue::Float))
            .unwrap_or_else(|_| KdlValue::String(s.into())),
    })
}

/// [`parse_value`], and a JSON list of strings (`["a","b"]`) as one value each — how a client
/// sends a list key. `[]` is an empty list, not unset.
pub fn parse_values(s: &str) -> Option<Vec<KdlValue>> {
    if let Ok(list) = serde_json::from_str::<Vec<String>>(s.trim()) {
        return Some(list.into_iter().map(KdlValue::String).collect());
    }
    parse_value(s).map(|v| vec![v])
}

pub(crate) fn set_at(
    main: &Path,
    ui: &Path,
    key: &str,
    values: Option<Vec<KdlValue>>,
) -> Result<(), ConfigError> {
    edit(main, ui, |doc| {
        let mut segs: Vec<&str> = key.split('.').collect();
        let last = segs.pop().unwrap_or_default();
        let mut doc = doc;
        let mut segs = segs.into_iter();
        while let Some(seg) = segs.next() {
            let n = if KEYED_NODES.contains(&seg)
                && let Some(id) = segs.next()
            {
                keyed(doc, seg, id)
            } else {
                node(doc, seg)
            };
            doc = n.ensure_children();
        }
        put(doc, last, values);
    })
}

fn edit(main: &Path, ui: &Path, f: impl FnOnce(&mut KdlDocument)) -> Result<(), ConfigError> {
    let old = std::fs::read_to_string(ui).ok();
    let mut doc: KdlDocument = match &old {
        Some(s) => s.parse().map_err(|_| ConfigError {
            file: PathBuf::from(ui),
            line: 0,
            col: 0,
            message: "ui.kdl is not valid KDL; delete it to start over".into(),
        })?,
        None => KdlDocument::new(),
    };
    f(&mut doc);
    doc.autoformat();
    let text = format!("{HEADER}{}", doc.to_string().trim_start_matches(HEADER));
    if let Some(dir) = ui.parent() {
        std::fs::create_dir_all(dir).map_err(|e| ConfigError::io(dir.into(), &e))?;
    }
    std::fs::write(ui, &text).map_err(|e| ConfigError::io(ui.into(), &e))?;

    // Checked against the whole config, not just ui.kdl: a value is only wrong in context.
    let check = if main.exists() {
        crate::kdl::load(main).map(drop)
    } else {
        crate::kdl::load(ui).map(drop)
    };
    if let Err(e) = check {
        let _ = match old {
            Some(o) => std::fs::write(ui, o),
            None => std::fs::remove_file(ui),
        };
        return Err(e);
    }
    Ok(())
}

/// The child node `name`, created if missing.
fn node<'a>(doc: &'a mut KdlDocument, name: &str) -> &'a mut KdlNode {
    let nodes = doc.nodes_mut();
    match nodes.iter().position(|n| n.name().value() == name) {
        Some(i) => &mut nodes[i],
        None => {
            nodes.push(KdlNode::new(name));
            nodes.last_mut().expect("just pushed")
        }
    }
}

/// The `name "<id>"` node, created if missing.
fn keyed<'a>(doc: &'a mut KdlDocument, name: &str, id: &str) -> &'a mut KdlNode {
    let nodes = doc.nodes_mut();
    let at = nodes.iter().position(|n| {
        n.name().value() == name && matches!(n.get(0), Some(KdlValue::String(s)) if s == id)
    });
    match at {
        Some(i) => &mut nodes[i],
        None => {
            let mut n = KdlNode::new(name);
            n.push(KdlEntry::new(id));
            nodes.push(n);
            nodes.last_mut().expect("just pushed")
        }
    }
}

fn put(doc: &mut KdlDocument, name: &str, values: Option<Vec<KdlValue>>) {
    match values {
        Some(vs) => {
            let n = node(doc, name);
            n.entries_mut().clear();
            n.clear_children();
            for v in vs {
                n.push(KdlEntry::new(v));
            }
        }
        None => doc.nodes_mut().retain(|n| n.name().value() != name),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn writes_merge_over_config_and_bad_ones_are_undone() {
        let dir = std::env::temp_dir().join(format!("wado-ui-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (main, ui) = (dir.join("config.kdl"), dir.join("ui.kdl"));
        std::fs::write(&main, "stream { max-fps 120 }").unwrap();

        set_at(&main, &ui, "stream.max-fps", parse_values("90")).unwrap();
        set_at(&main, &ui, "input.repeat-rate", parse_values("30")).unwrap();
        let c = crate::kdl::load(&main).unwrap();
        assert_eq!(c.stream.max_fps, Some(90), "ui.kdl wins over config.kdl");
        assert_eq!(c.input.repeat_rate, Some(30));

        // A typo is refused, and the file is back as it was.
        let before = std::fs::read_to_string(&ui).unwrap();
        assert!(set_at(&main, &ui, "stream.max-fsp", parse_values("1")).is_err());
        assert_eq!(std::fs::read_to_string(&ui).unwrap(), before);

        // Unset.
        set_at(&main, &ui, "stream.max-fps", None).unwrap();
        assert_eq!(crate::kdl::load(&main).unwrap().stream.max_fps, Some(120));
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_device_key_writes_into_its_keyed_block() {
        let dir = std::env::temp_dir().join(format!("wado-ui-dev-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let (main, ui) = (dir.join("config.kdl"), dir.join("ui.kdl"));
        std::fs::write(&main, "").unwrap();
        std::fs::write(&ui, "device \"k1\" { name \"Pixel\"; }").unwrap();

        set_at(
            &main,
            &ui,
            "device.k1.trackpad.rail",
            parse_values("actions"),
        )
        .unwrap();
        set_at(
            &main,
            &ui,
            "device.k1.trackpad.quick",
            parse_values(r#"["fullscreen","play-pause"]"#),
        )
        .unwrap();
        set_at(
            &main,
            &ui,
            "device.k2.trackpad.gestures.edge-left",
            parse_values("lock"),
        )
        .unwrap();
        let c = crate::kdl::load(&main).unwrap();
        let k1 = &c.device["k1"];
        assert_eq!(k1.name.as_deref(), Some("Pixel"), "the block is shared");
        let t = k1.trackpad.clone().unwrap();
        assert_eq!(t.rail, "actions");
        assert_eq!(t.quick, ["fullscreen", "play-pause"]);
        assert_eq!(
            c.device["k2"].trackpad.clone().unwrap().gestures["edge-left"],
            "lock"
        );

        // An emptied list stays empty rather than falling back to the default.
        set_at(&main, &ui, "device.k1.trackpad.quick", parse_values("[]")).unwrap();
        let c = crate::kdl::load(&main).unwrap();
        assert!(c.device["k1"].trackpad.clone().unwrap().quick.is_empty());

        // A bad value is refused like any other.
        assert!(set_at(&main, &ui, "device.k1.trackpad.rail", parse_values("left")).is_err());

        // The console's block is written the same way, beside the trackpad's.
        set_at(&main, &ui, "device.k1.console.face", parse_values("keys")).unwrap();
        set_at(&main, &ui, "device.k1.console.open", parse_values("full")).unwrap();
        let c = crate::kdl::load(&main).unwrap();
        let k = c.device["k1"].console.clone().unwrap();
        assert_eq!((k.open.as_str(), k.face.as_str()), ("full", "keys"));
        assert!(
            c.device["k1"].trackpad.is_some(),
            "the trackpad block is kept"
        );
        assert!(set_at(&main, &ui, "device.k1.console.face", parse_values("mouse")).is_err());

        // And the size's: a number, a switch and a word.
        set_at(&main, &ui, "device.k1.size.text", parse_values("115")).unwrap();
        set_at(&main, &ui, "device.k1.size.follow", parse_values("true")).unwrap();
        set_at(
            &main,
            &ui,
            "device.k1.size.interface",
            parse_values("compact"),
        )
        .unwrap();
        let c = crate::kdl::load(&main).unwrap();
        let s = c.device["k1"].size.clone().unwrap();
        assert_eq!(
            (s.text, s.follow, s.interface.as_str()),
            (115, true, "compact")
        );
        assert!(set_at(&main, &ui, "device.k1.size.text", parse_values("113")).is_err());
        std::fs::remove_dir_all(dir).ok();
    }

    #[test]
    fn a_json_list_is_one_value_each() {
        assert_eq!(
            parse_values(r#"["a", "b"]"#),
            Some(vec![
                KdlValue::String("a".into()),
                KdlValue::String("b".into())
            ])
        );
        assert_eq!(parse_values("[]"), Some(vec![]));
        assert_eq!(parse_values("90"), Some(vec![KdlValue::Integer(90)]));
        assert_eq!(parse_values(""), None);
    }

    #[test]
    fn values_parse_as_kdl_types() {
        assert_eq!(parse_value("90"), Some(KdlValue::Integer(90)));
        assert_eq!(parse_value("true"), Some(KdlValue::Bool(true)));
        assert_eq!(
            parse_value("wss://x"),
            Some(KdlValue::String("wss://x".into()))
        );
        assert_eq!(parse_value(""), None);
    }
}
