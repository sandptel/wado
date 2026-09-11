//! Discovering launchable applications from XDG desktop entries.
//!
//! One job: turn the `.desktop` files installed on this machine into a list of name/command
//! pairs. Nothing here knows about HTTP, the relay, or the compositor.
//!
//! A `.desktop` file is an INI file, and everything needed from it is two keys in one group.
//! Reading them directly is about thirty lines; a spec-complete desktop-entry crate would be a
//! dependency for icons, MIME associations and localisation that nothing here uses.
//!
//! Consciously *not* implemented, because none of it changes which entries a person sees in a
//! launcher list: `OnlyShowIn`/`NotShowIn` (there is no desktop environment name to match
//! against here), `DBusActivatable`, desktop actions, and localised `Name[xx]` variants.

use std::{collections::BTreeMap, fs, path::PathBuf};

use wado_protocol::AppEntry;

/// Every `applications` directory to scan, in XDG precedence order (most specific first).
///
/// Reads the environment rather than hard-coding `/usr/share`: on NixOS — where this is
/// developed — applications live in profile paths that a hard-coded list would miss entirely.
fn search_dirs() -> Vec<PathBuf> {
    let home = std::env::var_os("HOME").map(PathBuf::from);
    let data_home = std::env::var_os("XDG_DATA_HOME")
        .map(PathBuf::from)
        .or_else(|| home.map(|h| h.join(".local/share")));

    let data_dirs = std::env::var("XDG_DATA_DIRS")
        .unwrap_or_else(|_| "/usr/local/share:/usr/share".to_string());

    data_home
        .into_iter()
        .chain(
            data_dirs
                .split(':')
                .filter(|p| !p.is_empty())
                .map(PathBuf::from),
        )
        .map(|p| p.join("applications"))
        .collect()
}

/// Strip the `Exec=` field codes a launcher is expected to substitute.
///
/// `%f %F %u %U` are file/URL arguments, `%i %c %k` are icon/name/path expansions, and the
/// deprecated `%d %D %n %N %v %m` must be ignored outright. Nothing is being opened here, so
/// every one of them is dropped — passing `%U` through verbatim would hand the app a literal
/// "%U" as its first argument, which some apps treat as a filename and fail on.
///
/// `%%` is an escaped literal percent and survives as `%`.
fn strip_field_codes(exec: &str) -> String {
    let mut out = String::with_capacity(exec.len());
    let mut chars = exec.chars();
    while let Some(c) = chars.next() {
        if c != '%' {
            out.push(c);
            continue;
        }
        match chars.next() {
            Some('%') => out.push('%'),
            Some(_) => {}          // a field code: drop the pair
            None => out.push('%'), // trailing '%' — leave it rather than invent a meaning
        }
    }
    out.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Parse one desktop file. `None` when it is not something a person can launch.
fn parse_entry(text: &str) -> Option<AppEntry> {
    let mut name = None;
    let mut exec = None;
    let mut is_application = false;
    let mut hidden = false;

    // Only the `[Desktop Entry]` group counts; later groups are per-action overrides that
    // would otherwise overwrite the real Exec with an action's variant of it.
    let mut in_group = false;
    for line in text.lines() {
        let line = line.trim();
        if line.starts_with('[') {
            in_group = line == "[Desktop Entry]";
            continue;
        }
        if !in_group || line.starts_with('#') {
            continue;
        }
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        match (key.trim(), value.trim()) {
            ("Type", v) => is_application = v == "Application",
            ("Name", v) => name = Some(v.to_string()),
            ("Exec", v) => exec = Some(strip_field_codes(v)),
            // NoDisplay means "installed, but not for humans to pick" — mime handlers,
            // helper stubs. Hidden means "deleted" per the spec.
            ("NoDisplay" | "Hidden", "true") => hidden = true,
            _ => {}
        }
    }

    let (name, exec) = (name?, exec?);
    (is_application && !hidden && !name.is_empty() && !exec.is_empty())
        .then_some(AppEntry { name, exec })
}

/// Scan the system for launchable applications, sorted by name.
///
/// Entries are keyed by desktop-file id so a user override in `~/.local/share` shadows the
/// system copy rather than appearing twice — which is the entire reason the search order is
/// most-specific-first.
pub fn discover() -> Vec<AppEntry> {
    let mut by_id: BTreeMap<String, AppEntry> = BTreeMap::new();

    for dir in search_dirs() {
        let Ok(entries) = fs::read_dir(&dir) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.extension().is_none_or(|e| e != "desktop") {
                continue;
            }
            let Some(id) = path.file_name().map(|n| n.to_string_lossy().into_owned()) else {
                continue;
            };
            if by_id.contains_key(&id) {
                continue; // an earlier, higher-precedence directory already provided it
            }
            if let Some(app) = fs::read_to_string(&path)
                .ok()
                .as_deref()
                .and_then(parse_entry)
            {
                by_id.insert(id, app);
            }
        }
    }

    let mut apps: Vec<AppEntry> = by_id.into_values().collect();
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn strips_field_codes_but_keeps_escaped_percent() {
        assert_eq!(strip_field_codes("firefox %u"), "firefox");
        assert_eq!(strip_field_codes("gimp-2.10 %U"), "gimp-2.10");
        assert_eq!(strip_field_codes("app -i %i -c %c %f"), "app -i -c");
        assert_eq!(strip_field_codes("printf 100%%"), "printf 100%");
        assert_eq!(strip_field_codes("weston-terminal"), "weston-terminal");
    }

    #[test]
    fn reads_name_and_exec_from_the_desktop_entry_group() {
        let app = parse_entry(
            "[Desktop Entry]\nType=Application\nName=Files\nExec=nautilus %U\nIcon=folder\n",
        )
        .expect("a normal entry should parse");
        assert_eq!(
            app,
            AppEntry {
                name: "Files".into(),
                exec: "nautilus".into()
            }
        );
    }

    #[test]
    fn later_groups_do_not_overwrite_exec() {
        // Desktop actions are a separate group with their own Exec. Reading the whole file
        // flat would launch "nautilus --new-window" under the name "Files".
        let app = parse_entry(
            "[Desktop Entry]\nType=Application\nName=Files\nExec=nautilus\n\
             [Desktop Action new-window]\nName=New Window\nExec=nautilus --new-window\n",
        )
        .unwrap();
        assert_eq!(app.exec, "nautilus");
        assert_eq!(app.name, "Files");
    }

    #[test]
    fn skips_what_a_person_should_not_be_offered() {
        // Not an application.
        assert!(parse_entry("[Desktop Entry]\nType=Link\nName=Web\nExec=x\n").is_none());
        // Installed but not for picking: mime handlers and helper stubs.
        assert!(
            parse_entry("[Desktop Entry]\nType=Application\nName=X\nExec=x\nNoDisplay=true\n")
                .is_none()
        );
        // "Deleted" per the spec.
        assert!(
            parse_entry("[Desktop Entry]\nType=Application\nName=X\nExec=x\nHidden=true\n")
                .is_none()
        );
        // Nothing to run, or nothing to show.
        assert!(parse_entry("[Desktop Entry]\nType=Application\nName=X\n").is_none());
        assert!(parse_entry("[Desktop Entry]\nType=Application\nExec=x\n").is_none());
    }
}
