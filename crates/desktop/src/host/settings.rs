// SPDX-License-Identifier: AGPL-3.0-only
//! Settings: config.kdl as fields. Edits are staged here and shown as "not applied"; Apply
//! writes each through `caps::config::write` (ui.kdl, checked against the whole config, undone
//! on error) and has every daemon reload. A refused value keeps its staged text and shows the
//! loader's words under its field.

use std::collections::BTreeMap;

use serde_json::Value;
use slint::ComponentHandle;

use super::Ctl;
use crate::{
    AppWindow, ConfigField, ConfigSection, Host,
    caps::{
        self,
        config::{Field, Kind, SECTIONS, Staged},
        watch::HostSnapshot,
    },
    models::{strings, sync},
};

#[derive(Default)]
pub struct State {
    staged: BTreeMap<String, Staged>,
    errors: BTreeMap<String, String>,
    /// The section's fields as last read, for staging against.
    fields: Vec<Field>,
}

pub fn sections(ui: &AppWindow) {
    with_models(|m| {
        sync(
            &m.sections,
            SECTIONS
                .iter()
                .map(|s| ConfigSection {
                    id: s.id.into(),
                    name: s.name.into(),
                    icon: icon(s.icon).into(),
                    blurb: s.blurb.into(),
                    changed: 0,
                })
                .collect(),
        )
    });
    let (main, ui_path) = caps::config::paths();
    let h = ui.global::<Host>();
    h.set_config_path(main.into());
    h.set_ui_path(ui_path.into());
}

fn with_models(f: impl FnOnce(&super::Models)) {
    super::with(|c| f(&c.models));
}

/// The icon path for a section's icon name — the same strings as `Icons` in the UI.
fn icon(name: &str) -> &'static str {
    match name {
        "server" => {
            "M4 3h16a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM4 13h16a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2v-4a2 2 0 0 1 2-2zM6 7h.01M6 17h.01"
        }
        "lock" => {
            "M6 11h12a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-6a2 2 0 0 1 2-2zM8 11V7a4 4 0 0 1 8 0v4"
        }
        "monitor" => {
            "M4 3h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM8 21h8M12 17v4"
        }
        "layers" => "M12 2 2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5",
        "term" => "M4 17l6-6-6-6M12 19h8",
        "folder" => "M3 7a2 2 0 0 1 2-2h4l2 2h8a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2z",
        "kbd" => {
            "M4 6h16a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2zM6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10"
        }
        "trackpad" => {
            "M5 4h14a2 2 0 0 1 2 2v12a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2V6a2 2 0 0 1 2-2zM3 15h18M12 15v5"
        }
        "pulse" => "M22 12h-4l-3 9L9 3l-3 9H2",
        _ => "",
    }
}

pub fn wire(ui: &AppWindow) {
    let h = ui.global::<Host>();
    let weak = ui.as_weak();
    h.on_pick_section(move |i| {
        let Some(ui) = weak.upgrade() else { return };
        ui.global::<Host>().set_section(i);
        super::with(|c| rebuild(&ui, c));
    });
    let weak = ui.as_weak();
    h.on_edit(move |key, text| {
        let Some(ui) = weak.upgrade() else { return };
        let t = text.trim().to_string();
        super::with(|c| {
            let kind = c
                .settings
                .fields
                .iter()
                .find(|f| f.key == key.as_str())
                .map(|f| f.kind.clone());
            let s = match kind {
                _ if t.is_empty() => Staged::Unset,
                Some(Kind::Int) => Staged::Number(t),
                _ => Staged::Text(t),
            };
            stage(&ui, c, &key, s);
        });
    });
    let weak = ui.as_weak();
    h.on_toggle(move |key, on| {
        let Some(ui) = weak.upgrade() else { return };
        super::with(|c| stage(&ui, c, &key, Staged::Bool(on)));
    });
    let weak = ui.as_weak();
    h.on_pick(move |key, i| {
        let Some(ui) = weak.upgrade() else { return };
        super::with(|c| {
            let Some(f) = c.settings.fields.iter().find(|f| f.key == key.as_str()) else {
                return;
            };
            let Kind::Enum(opts) = &f.kind else { return };
            let pick = opts.get(i as usize).cloned().unwrap_or_default();
            // An optional switch: unset · on · off.
            let s = match (opts.first().map(String::as_str), pick.as_str()) {
                (Some("unset"), "unset") => Staged::Unset,
                (Some("unset"), "on") => Staged::Bool(true),
                (Some("unset"), "off") => Staged::Bool(false),
                _ => Staged::Text(pick),
            };
            stage(&ui, c, &key, s);
        });
    });
    let weak = ui.as_weak();
    h.on_list_add(move |key, item| {
        let Some(ui) = weak.upgrade() else { return };
        super::with(|c| {
            let mut items = current_list(c, &key);
            items.push(item.trim().to_string());
            stage(&ui, c, &key, Staged::List(items));
        });
    });
    let weak = ui.as_weak();
    h.on_list_remove(move |key, i| {
        let Some(ui) = weak.upgrade() else { return };
        super::with(|c| {
            let mut items = current_list(c, &key);
            if (i as usize) < items.len() {
                items.remove(i as usize);
            }
            stage(&ui, c, &key, Staged::List(items));
        });
    });
    let weak = ui.as_weak();
    h.on_clear(move |key| {
        let Some(ui) = weak.upgrade() else { return };
        let r = caps::config::write(&key, &Staged::Unset);
        match r {
            Ok(()) => {
                caps::daemon::reload_all();
                crate::toast::say(&ui, &format!("{key} is back to config.kdl's value"), "good");
            }
            Err(e) => crate::toast::say(&ui, &e, "bad"),
        }
        super::with(|c| {
            c.settings.staged.remove(key.as_str());
            refresh_now(&ui, c);
        });
    });
    let weak = ui.as_weak();
    h.on_discard(move || {
        let Some(ui) = weak.upgrade() else { return };
        super::with(|c| {
            c.settings.staged.clear();
            c.settings.errors.clear();
            rebuild(&ui, c);
        });
    });
    let weak = ui.as_weak();
    h.on_apply(move || {
        let Some(ui) = weak.upgrade() else { return };
        super::with(|c| apply_staged(&ui, c));
    });
}

fn current_list(c: &Ctl, key: &str) -> Vec<String> {
    if let Some(Staged::List(l)) = c.settings.staged.get(key) {
        return l.clone();
    }
    c.settings
        .fields
        .iter()
        .find(|f| f.key == key)
        .and_then(|f| f.value.as_array().cloned())
        .unwrap_or_default()
        .into_iter()
        .filter_map(|v| v.as_str().map(str::to_string))
        .collect()
}

fn stage(ui: &AppWindow, c: &mut Ctl, key: &str, s: Staged) {
    let Some(f) = c.settings.fields.iter().find(|f| f.key == key) else {
        return;
    };
    match caps::config::check(f, &s) {
        Ok(()) => {
            c.settings.errors.remove(key);
        }
        Err(e) => {
            c.settings.errors.insert(key.to_string(), e);
        }
    }
    // Staging the value already in force is no change at all.
    if same(&f.value, &s) {
        c.settings.staged.remove(key);
    } else {
        c.settings.staged.insert(key.to_string(), s);
    }
    rebuild(ui, c);
}

fn same(v: &Value, s: &Staged) -> bool {
    match (v, s) {
        (Value::Null, Staged::Unset) => true,
        (Value::Bool(a), Staged::Bool(b)) => a == b,
        (Value::Number(a), Staged::Number(b)) => a.to_string() == b.trim(),
        (Value::String(a), Staged::Text(b)) => a == b,
        (Value::Array(a), Staged::List(b)) => {
            a.len() == b.len() && a.iter().zip(b).all(|(x, y)| x.as_str() == Some(y))
        }
        _ => false,
    }
}

fn apply_staged(ui: &AppWindow, c: &mut Ctl) {
    if c.settings
        .errors
        .keys()
        .any(|k| c.settings.staged.contains_key(k))
    {
        ui.global::<Host>()
            .set_config_error("Fix the fields marked in red first.".into());
        return;
    }
    let staged = std::mem::take(&mut c.settings.staged);
    let mut ok = 0;
    for (key, s) in staged {
        match caps::config::write(&key, &s) {
            Ok(()) => ok += 1,
            Err(e) => {
                c.settings.errors.insert(key.clone(), e);
                c.settings.staged.insert(key, s);
            }
        }
    }
    if ok > 0 {
        caps::daemon::reload_all();
    }
    let h = ui.global::<Host>();
    if c.settings.staged.is_empty() {
        h.set_config_error("".into());
        crate::toast::say(
            ui,
            &format!(
                "Applied {ok} change{} — written to ui.kdl, the daemons reloaded",
                if ok == 1 { "" } else { "s" }
            ),
            "good",
        );
    } else {
        h.set_config_error("The config refused a value — see the field.".into());
        crate::toast::say(ui, "Some changes were refused — see the fields", "bad");
    }
    refresh_now(ui, c);
}

/// Re-read the config now (a write just happened) rather than at the next snapshot.
fn refresh_now(ui: &AppWindow, c: &mut Ctl) {
    c.poke();
    rebuild(ui, c);
}

pub fn apply(ui: &AppWindow, ctl: &mut Ctl, s: &HostSnapshot) {
    let h = ui.global::<Host>();
    if let Err(e) = &s.config {
        h.set_config_error(format!("config.kdl does not load: {e}").into());
    }
    rebuild_from(ui, ctl, s);
}

fn rebuild(ui: &AppWindow, c: &mut Ctl) {
    if let Some(s) = c.last().cloned() {
        rebuild_from(ui, c, &s);
    } else {
        // No snapshot yet: read the config here (screenshots, first frame).
        if let Ok(cfg) = caps::config::load() {
            let section = ui.global::<Host>().get_section() as usize;
            c.settings.fields = caps::config::fields(
                &cfg,
                SECTIONS[section.min(SECTIONS.len() - 1)].id,
                &caps::config::overridden(),
            );
            show(ui, c);
        }
    }
}

fn rebuild_from(ui: &AppWindow, c: &mut Ctl, s: &HostSnapshot) {
    let Ok(cfg) = &s.config else { return };
    let section = (ui.global::<Host>().get_section() as usize).min(SECTIONS.len() - 1);
    c.settings.fields = caps::config::fields(cfg, SECTIONS[section].id, &s.overridden);
    show(ui, c);
}

fn show(ui: &AppWindow, c: &mut Ctl) {
    let sig = format!(
        "{:?}{:?}{:?}",
        c.settings.fields, c.settings.staged, c.settings.errors
    );
    if !c.changed("fields", sig) {
        return;
    }
    let st = &c.settings;
    let rows: Vec<ConfigField> = st.fields.iter().map(|f| row(f, st)).collect();
    sync(&c.models.fields, rows);
    // Changed-count per section, for the tabs.
    let counts: Vec<ConfigSection> = SECTIONS
        .iter()
        .map(|s| ConfigSection {
            id: s.id.into(),
            name: s.name.into(),
            icon: icon(s.icon).into(),
            blurb: s.blurb.into(),
            changed: st
                .staged
                .keys()
                .filter(|k| k.starts_with(&format!("{}.", s.id)))
                .count() as i32,
        })
        .collect();
    sync(&c.models.sections, counts);
    ui.global::<Host>().set_dirty(st.staged.len() as i32);
    if st.staged.is_empty() {
        ui.global::<Host>().set_config_error("".into());
    }
}

fn row(f: &Field, st: &State) -> ConfigField {
    let staged = st.staged.get(&f.key);
    let text_of = |v: &Value| match v {
        Value::Null => String::new(),
        Value::String(s) => s.clone(),
        other => other.to_string(),
    };
    let value = match staged {
        Some(Staged::Unset) => String::new(),
        Some(Staged::Number(n)) | Some(Staged::Text(n)) => n.clone(),
        Some(Staged::Bool(b)) => b.to_string(),
        Some(Staged::List(_)) | None => text_of(&f.value),
    };
    let on = match staged {
        Some(Staged::Bool(b)) => *b,
        _ => f.value.as_bool().unwrap_or(false),
    };
    let items: Vec<String> = match (staged, &f.kind) {
        (Some(Staged::List(l)), _) => l.clone(),
        (_, Kind::Map) => match &f.value {
            Value::Object(m) => m
                .iter()
                .map(|(k, v)| format!("{k} = {}", text_of(v)))
                .collect(),
            _ => Vec::new(),
        },
        _ => f
            .value
            .as_array()
            .map(|a| a.iter().map(text_of).collect())
            .unwrap_or_default(),
    };
    let (kind, options, index) = match &f.kind {
        Kind::Bool => ("bool", Vec::new(), 0),
        Kind::Enum(opts) => {
            let shown: Vec<String> = opts
                .iter()
                .map(|o| match o.as_str() {
                    "unset" => "Device decides".to_string(),
                    "on" => "On".to_string(),
                    "off" => "Off".to_string(),
                    other => other.to_string(),
                })
                .collect();
            let current = match (opts.first().map(String::as_str), staged) {
                (Some("unset"), Some(Staged::Unset)) => "unset".to_string(),
                (Some("unset"), Some(Staged::Bool(b))) => if *b { "on" } else { "off" }.to_string(),
                (Some("unset"), None) => match f.value {
                    Value::Bool(true) => "on".into(),
                    Value::Bool(false) => "off".into(),
                    _ => "unset".into(),
                },
                (_, Some(Staged::Text(t))) => t.clone(),
                _ => text_of(&f.value),
            };
            let i = opts.iter().position(|o| *o == current).unwrap_or(0);
            ("enum", shown, i as i32)
        }
        Kind::Int => ("int", Vec::new(), 0),
        Kind::Text => ("text", Vec::new(), 0),
        Kind::List => ("list", Vec::new(), 0),
        Kind::Map => ("map", Vec::new(), 0),
    };
    ConfigField {
        key: f.key.as_str().into(),
        name: f.name.as_str().into(),
        hint: f.hint.as_str().into(),
        kind: kind.into(),
        value: value.into(),
        placeholder: if f.example.is_empty() {
            "unset".into()
        } else {
            format!("unset — e.g. {}", f.example).into()
        },
        options: strings(options),
        option_index: index,
        on,
        items: strings(items),
        when: if f.restart { "restart" } else { "live" }.into(),
        owner_only: f.owner_only,
        overridden: f.overridden,
        dirty: staged.is_some(),
        error: st.errors.get(&f.key).cloned().unwrap_or_default().into(),
    }
}
