// SPDX-License-Identifier: AGPL-3.0-only
//! config.kdl as fields, through `wado_config` — never a second schema.
//!
//! - **What exists** is the live `Config` itself, serialised: every section's keys, with their
//!   current values and JSON types. A key added to the schema shows up here without a change.
//! - **What it means** comes from the commented starting config (`wado_config::DEFAULT_KDL`):
//!   each `// key example   // what it does` line gives the hint, the example (the placeholder
//!   of an unset key) and, where it lists `"a" | "b"`, the choices.
//! - **Writing** goes through `wado_config::ui_file`, which loads the whole config after the
//!   write and undoes one that would break it; its error (with line and column) is the field's.
//! - **Who may change it from a phone** is `wado_config::tier`; **when it applies** mirrors the
//!   daemon's `needs_restart` list.

use std::{collections::BTreeSet, sync::Arc};

use serde_json::Value;
use wado_config::{Config, schema::gestures, tier};

pub struct Section {
    pub id: &'static str,
    pub name: &'static str,
    pub icon: &'static str,
    pub blurb: &'static str,
}

/// Every section, in the order config.kdl writes them. `device` (each phone's own preferences,
/// written by the phones) and `window-rule` (a list of match blocks) stay in the file.
pub const SECTIONS: &[Section] = &[
    Section {
        id: "server",
        name: "Server",
        icon: "server",
        blurb: "How the daemon is reached. Most of this is read once, when it starts.",
    },
    Section {
        id: "security",
        name: "Security",
        icon: "lock",
        blurb: "Who gets in and what a new device starts with.",
    },
    Section {
        id: "stream",
        name: "Stream",
        icon: "monitor",
        blurb: "The host's say over encoding. Devices choose freely inside these limits.",
    },
    Section {
        id: "session",
        name: "Session",
        icon: "layers",
        blurb: "What a session's applications get.",
    },
    Section {
        id: "shells",
        name: "Shells",
        icon: "term",
        blurb: "The console's terminals.",
    },
    Section {
        id: "files",
        name: "Files",
        icon: "folder",
        blurb: "What the file manager may reach. Which devices may use it is a grant.",
    },
    Section {
        id: "input",
        name: "Input",
        icon: "kbd",
        blurb: "Keyboard and pointer behaviour pinned for every device. Unset: each device decides.",
    },
    Section {
        id: "trackpad",
        name: "Trackpad",
        icon: "trackpad",
        blurb: "A phone as this computer's touchpad and keyboard.",
    },
    Section {
        id: "binds",
        name: "Shortcuts",
        icon: "kbd",
        blurb: "Keyboard shortcuts the compositor keeps for itself.",
    },
    Section {
        id: "gestures",
        name: "Gestures",
        icon: "pulse",
        blurb: "What three-finger swipes do on a device.",
    },
];

#[derive(Debug, Clone, PartialEq)]
pub enum Kind {
    Bool,
    /// `None` = unset is one of the choices (an optional value the device decides).
    Enum(Vec<String>),
    Int,
    Text,
    List,
    /// Shown, edited in config.kdl.
    Map,
}

#[derive(Debug, Clone)]
pub struct Field {
    /// `stream.max-fps`.
    pub key: String,
    pub name: String,
    pub hint: String,
    pub kind: Kind,
    /// The value in force; `Null` when unset.
    pub value: Value,
    /// The starting config's example, for an unset key's placeholder.
    pub example: String,
    pub restart: bool,
    pub owner_only: bool,
    /// ui.kdl sets it, over config.kdl.
    pub overridden: bool,
}

/// Load the config the daemons would load, and make it the process's current one.
pub fn load() -> Result<Arc<Config>, String> {
    let l = wado_config::load_or_init().map_err(|e| e.to_string())?;
    wado_config::live::install(l.config);
    Ok(wado_config::live::current())
}

pub fn paths() -> (String, String) {
    (
        wado_config::paths::config_file().display().to_string(),
        wado_config::paths::ui_file().display().to_string(),
    )
}

/// The fields of one section, in schema order.
/// `over` is what ui.kdl sets ([`overridden`]), read with the snapshot rather than per rebuild.
pub fn fields(cfg: &Config, section: &str, over: &BTreeSet<String>) -> Vec<Field> {
    let hints = hints();
    let Ok(all) = serde_json::to_value(cfg) else {
        return Vec::new();
    };
    let Some(Value::Object(sec)) = all.get(section) else {
        return Vec::new();
    };
    let mut out = Vec::new();
    // Sections that are themselves maps of key → action.
    if section == "gestures" {
        for name in gestures::NAMES {
            let key = format!("gestures.{name}");
            out.push(Field {
                name: words(name),
                hint: "What this swipe does.".into(),
                kind: Kind::Enum(gestures::ACTIONS.iter().map(|s| s.to_string()).collect()),
                value: sec.get(*name).cloned().unwrap_or(Value::Null),
                example: String::new(),
                restart: false,
                owner_only: tier::of(&key) == tier::Tier::Privileged,
                overridden: over.contains(&key),
                key,
            });
        }
        return out;
    }
    for (k, v) in sec {
        let key = format!("{section}.{k}");
        let (example, hint) = hints
            .iter()
            .find(|h| h.0 == key)
            .map(|h| (h.1.clone(), h.2.clone()))
            .unwrap_or_default();
        let kind = kind_of(section, k, v, &example, &hint);
        out.push(Field {
            name: if section == "binds" && k != "mod" {
                k.clone()
            } else {
                words(k)
            },
            hint: tidy_hint(&hint),
            kind,
            value: v.clone(),
            example: example.trim_matches('"').to_string(),
            restart: RESTART.contains(&key.as_str()),
            owner_only: tier::of(&key) == tier::Tier::Privileged,
            overridden: over.contains(&key),
            key,
        });
    }
    // Keys the schema leaves out when unset (`skip_serializing_if`) still have a line in the
    // starting config; list them so they can be set.
    for (key, example, hint) in hints
        .iter()
        .filter(|h| h.0.starts_with(&format!("{section}.")))
    {
        if out.iter().any(|f| &f.key == key) || section == "binds" {
            continue;
        }
        let k = key.rsplit('.').next().unwrap_or(key);
        out.push(Field {
            name: words(k),
            hint: tidy_hint(hint),
            kind: kind_of(section, k, &Value::Null, example, hint),
            value: Value::Null,
            example: example.trim_matches('"').to_string(),
            restart: RESTART.contains(&key.as_str()),
            owner_only: tier::of(key) == tier::Tier::Privileged,
            overridden: over.contains(key),
            key: key.clone(),
        });
    }
    // In the order the starting config writes them, which is the order they are explained in;
    // keys it does not mention keep the schema's (alphabetical) order after them.
    let at = |k: &str| hints.iter().position(|h| h.0 == k).unwrap_or(usize::MAX);
    out.sort_by_key(|f| at(&f.key));
    for f in &mut out {
        if let Kind::Enum(_) = f.kind {
            f.hint = enum_hint(&f.hint);
        }
    }
    out
}

/// An enum's hint without the list the control already shows: `"auto" (client decides) |
/// "hardware" | "software"` → `auto: client decides`; `how far …: "adaptive" (…) | …` → the words
/// before the colon, then each choice's note.
fn enum_hint(h: &str) -> String {
    if !h.contains('|') {
        return h.to_string();
    }
    let (lead, list) = h
        .rsplit_once(':')
        .filter(|(_, l)| l.contains('|'))
        .unwrap_or(("", h));
    let notes: Vec<String> = list
        .split('|')
        .filter_map(|p| {
            let p = p.trim();
            let name = p.trim_start_matches('"').split('"').next()?.to_string();
            let note = p.split_once('(')?.1.split(')').next()?.trim().to_string();
            Some(format!("{name}: {note}"))
        })
        .collect();
    let lead = lead.trim();
    match (lead.is_empty(), notes.is_empty()) {
        (true, true) => String::new(),
        (true, false) => tidy_hint(&notes.join(" · ")),
        (false, true) => tidy_hint(lead),
        (false, false) => format!("{} — {}", tidy_hint(lead), notes.join(" · ")),
    }
}

/// What the daemon reads once at startup (`server::config::watch::needs_restart`).
const RESTART: &[&str] = &[
    "server.listen",
    "server.relay",
    "server.instance",
    "server.remote-id",
    "server.udp-slice",
    "server.turn",
];

fn kind_of(section: &str, key: &str, v: &Value, example: &str, hint: &str) -> Kind {
    if section == "binds" {
        if key == "mod" {
            return Kind::Enum(
                ["super", "alt", "ctrl", "ctrl+alt"]
                    .map(String::from)
                    .to_vec(),
            );
        }
        return Kind::Enum(
            wado_config::schema::binds::ACTIONS
                .iter()
                .map(|s| s.to_string())
                .collect(),
        );
    }
    match v {
        Value::Bool(_) => Kind::Bool,
        Value::Number(_) => Kind::Int,
        Value::Array(_) => Kind::List,
        Value::Object(_) => Kind::Map,
        Value::String(_) => match choices(hint) {
            Some(c) => Kind::Enum(c),
            None => Kind::Text,
        },
        Value::Null => {
            let ex = example.trim();
            if ex.starts_with('{') {
                Kind::Map
            } else if ex == "#true" || ex == "#false" {
                Kind::Enum(vec!["unset".into(), "on".into(), "off".into()])
            } else if ex.parse::<f64>().is_ok() {
                Kind::Int
            } else {
                Kind::Text
            }
        }
    }
}

/// `"auto" (client decides) | "hardware" | "software"` → `[auto, hardware, software]`.
fn choices(hint: &str) -> Option<Vec<String>> {
    if !hint.contains('|') {
        return None;
    }
    let tail = hint.rsplit_once(':').map_or(hint, |(_, t)| t);
    let tail = if tail.contains('|') { tail } else { hint };
    let parts: Vec<&str> = tail.split('|').map(str::trim).collect();
    // Each choice is a quoted word, or a bare single word — `files-ro|files-rw shells …` is a
    // token list, not a choice.
    let opts: Option<Vec<String>> = parts
        .iter()
        .map(|p| match p.strip_prefix('"') {
            Some(rest) => rest.split('"').next().map(str::to_string),
            None if !p.contains(' ') => Some(p.trim_end_matches([';', ',', ')']).to_string()),
            None => None,
        })
        .collect();
    opts.filter(|o| o.len() >= 2 && o.iter().all(|s| !s.is_empty()))
}

/// Drop the choices list from a hint the control already shows.
fn tidy_hint(h: &str) -> String {
    let h = h.trim();
    let mut s = h.to_string();
    // A bare unit (`kbps`, `ms`) keeps its case.
    let unit = !h.contains(' ') && h.len() <= 4;
    if let Some(c) = s.chars().next().filter(|_| !unit) {
        s.replace_range(..c.len_utf8(), &c.to_uppercase().to_string());
    }
    s
}

/// `max-fps` → `Max fps`.
fn words(k: &str) -> String {
    let s = k.replace(['-', '_'], " ");
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => s,
    }
}

/// `(key, example, hint)` from the starting config's commented lines.
fn hints() -> Vec<(String, String, String)> {
    let mut out: Vec<(String, String, String)> = Vec::new();
    let mut section: Option<String> = None;
    for raw in wado_config::DEFAULT_KDL.lines() {
        let line = raw.trim();
        if let Some(name) = line.strip_suffix('{').map(str::trim) {
            if !name.starts_with("//") && !name.contains(' ') {
                section = Some(name.to_string());
                continue;
            }
        }
        if line == "}" {
            section = None;
            continue;
        }
        let Some(sec) = &section else { continue };
        let Some(body) = line.strip_prefix("//") else {
            continue;
        };
        let body = body.trim_start();
        // A continuation: `//     // more words`.
        if let Some(more) = body.strip_prefix("//") {
            if let Some(last) = out.last_mut() {
                last.2.push(' ');
                last.2.push_str(more.trim());
            }
            continue;
        }
        let (decl, hint) = body.split_once("//").unwrap_or((body, ""));
        let decl = decl.trim();
        let Some((key, example)) = decl.split_once(' ') else {
            continue;
        };
        if key
            .chars()
            .next()
            .is_some_and(|c| !c.is_ascii_lowercase() && c != 'M')
        {
            continue;
        }
        out.push((
            format!("{sec}.{key}"),
            example.trim().to_string(),
            hint.trim().to_string(),
        ));
    }
    out
}

/// Keys ui.kdl sets, as dotted paths (`stream.max-fps`, `device.<id>.trackpad.rail`).
pub fn overridden() -> BTreeSet<String> {
    let text = std::fs::read_to_string(wado_config::paths::ui_file()).unwrap_or_default();
    let mut out = BTreeSet::new();
    let mut stack: Vec<String> = Vec::new();
    for raw in text.lines() {
        let line = raw.split("//").next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let closes = line.matches('}').count();
        let opens = line.ends_with('{');
        let mut words = line.trim_end_matches('{').split_whitespace();
        let name = words.next().unwrap_or("").trim_matches('}');
        if opens {
            let mut seg = name.to_string();
            if wado_config::schema::KEYED_NODES.contains(&name) {
                if let Some(id) = words.next() {
                    seg = format!("{name}.{}", id.trim_matches('"'));
                }
            }
            stack.push(seg);
        } else if !name.is_empty() && !line.starts_with('}') {
            let mut path = stack.clone();
            path.push(name.to_string());
            out.insert(path.join("."));
        }
        for _ in 0..closes {
            stack.pop();
        }
    }
    out
}

/// A staged value, as the person wrote it.
#[derive(Debug, Clone, PartialEq)]
pub enum Staged {
    /// Unset (remove from ui.kdl).
    Unset,
    Bool(bool),
    Number(String),
    Text(String),
    List(Vec<String>),
}

/// Check a staged value against its field before writing: the shape a control can get wrong.
/// Context-dependent checks are `ui_file`'s, after the write.
pub fn check(f: &Field, s: &Staged) -> Result<(), String> {
    match (s, &f.kind) {
        (Staged::Number(n), _) if n.trim().parse::<i64>().is_err() => {
            Err("a whole number, or empty to leave it unset".into())
        }
        (Staged::Number(n), _) if n.trim().starts_with('-') => Err("it cannot be negative".into()),
        _ => Ok(()),
    }
}

/// Write one staged value to ui.kdl. Its error is what to show under the field.
pub fn write(key: &str, s: &Staged) -> Result<(), String> {
    use wado_config::ui_file;
    let r = match s {
        Staged::Unset => ui_file::set(key, None),
        Staged::Bool(b) => {
            ui_file::set(key, ui_file::parse_value(if *b { "true" } else { "false" }))
        }
        Staged::Number(n) => ui_file::set(key, ui_file::parse_value(n)),
        // Through a one-item JSON list, so `"1"` stays a string rather than becoming a number.
        Staged::Text(t) => ui_file::set_values(
            key,
            ui_file::parse_values(&serde_json::to_string(&[t]).unwrap_or_default()),
        ),
        Staged::List(items) => ui_file::set_values(
            key,
            ui_file::parse_values(&serde_json::to_string(items).unwrap_or_default()),
        ),
    };
    r.map_err(|e| e.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_starting_config_explains_every_section() {
        let h = hints();
        assert!(
            h.iter()
                .any(|(k, ex, _)| k == "stream.max-fps" && ex == "120")
        );
        let enc = h.iter().find(|(k, ..)| k == "stream.encoder").unwrap();
        assert_eq!(
            choices(&enc.2).unwrap(),
            vec!["auto", "hardware", "software"]
        );
        let delay = h.iter().find(|(k, ..)| k == "trackpad.delay").unwrap();
        assert_eq!(choices(&delay.2).unwrap(), vec!["adaptive", "fixed"]);
    }

    #[test]
    fn every_section_has_fields() {
        let cfg = Config::default();
        for s in SECTIONS {
            assert!(
                !fields(&cfg, s.id, &BTreeSet::new()).is_empty(),
                "{} is empty",
                s.id
            );
        }
        let stream = fields(&cfg, "stream", &BTreeSet::new());
        let enc = stream.iter().find(|f| f.key == "stream.encoder").unwrap();
        assert!(matches!(&enc.kind, Kind::Enum(o) if o.len() == 3));
        let fps = stream.iter().find(|f| f.key == "stream.max-fps").unwrap();
        assert_eq!(fps.kind, Kind::Int);
        assert_eq!(fps.example, "120");
        let relay = fields(&cfg, "server", &BTreeSet::new());
        assert!(relay.iter().any(|f| f.key == "server.relay" && f.restart));
    }
}
