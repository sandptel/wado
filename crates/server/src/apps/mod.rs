//! Discovering launchable applications from XDG desktop entries.
//!
//! One job: turn the `.desktop` files installed on this machine into a list a client can show.
//! Nothing here knows about HTTP, the relay, or the compositor.
//!
//! Split three ways, because they fail differently and are tested differently:
//!
//! | | |
//! |---|---|
//! | this file | *which* files to read, and which copy of a duplicate wins |
//! | [`desktop`] | what one file means — pure, string in / entry out |
//! | [`icons`] | turning `Icon=` into something a browser can draw |
//! | [`running`] | which of them the session has a live process for |

pub mod desktop;
pub mod icons;
pub mod running;

use std::{collections::BTreeMap, fs, path::PathBuf};

use wado_protocol::AppEntry;

use desktop::parse_entry;

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
            if let Some(entry) = fs::read_to_string(&path)
                .ok()
                .as_deref()
                .and_then(parse_entry)
            {
                // The one step that touches the disk beyond reading the entry itself, and the
                // reason it is here rather than in the parser.
                let icon = entry.icon.as_deref().and_then(icons::resolve);
                by_id.insert(
                    id,
                    AppEntry {
                        name: entry.name,
                        exec: entry.exec,
                        icon,
                        // Filled in by `running::mark` when the list is answered; discovery
                        // cannot know it.
                        running: false,
                    },
                );
            }
        }
    }

    let mut apps: Vec<AppEntry> = by_id.into_values().collect();
    apps.sort_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
    apps
}
