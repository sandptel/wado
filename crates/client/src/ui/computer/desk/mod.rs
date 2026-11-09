//! A Mac's own apps, with no session (`desk_apps`, `crate::host_caps::desk_apps`): on its page
//! the Apps dock stands where Sessions would, and "All apps" pushes the Apps page.
//!
//! | file | job |
//! |---|---|
//! | this | the order things go in, and what a tap or a menu item does — pure, tested |
//! | [`tile`] | one app's icon, and its menu |
//! | [`dock`] | the card: the Mac's own Dock, then what else runs |
//! | [`page`] | Apps: search, Open now, A–Z |
//!
//! The list is `live.apps`, filled by `js/relay.js` the way the session's app drawer is, icons and
//! their cache included; `js/desk_apps.js` asks for it every 3 s while it is on screen, sends the
//! actions and turns a long press into [`MENU`].

pub mod dock;
pub mod page;
pub mod tile;

use dioxus::prelude::*;
use wado_protocol::AppEntry;

/// The app whose menu is up, and whether it is asking to confirm Force Quit.
#[derive(Clone, Debug, PartialEq)]
pub struct Menu {
    pub exec: String,
    pub confirm: bool,
}

pub static MENU: GlobalSignal<Option<Menu>> = Signal::global(|| None);
/// Tapped open and not running yet: these bounce (`js/desk_apps.js` keeps it, at most 10 s).
pub static LAUNCHING: GlobalSignal<Vec<String>> = Signal::global(Vec::new);

/// `deskAppMenu` / `deskLaunching` from `js/desk_apps.js`.
pub fn on_event(kind: &str, msg: &serde_json::Value) {
    match kind {
        "deskAppMenu" => {
            let exec = msg.get("exec").and_then(|v| v.as_str()).unwrap_or("");
            *MENU.write() = Some(Menu {
                exec: exec.to_string(),
                confirm: false,
            });
        }
        "deskLaunching" => {
            *LAUNCHING.write() = msg
                .get("execs")
                .and_then(|v| serde_json::from_value(v.clone()).ok())
                .unwrap_or_default();
        }
        _ => {}
    }
}

/// Send `action` for `exec`.
pub fn act(exec: &str, action: &str, running: bool) {
    crate::bridge::call(format!(
        "window.__wado.deskAppDo({}, {}, {running});",
        crate::bridge::js(&exec),
        crate::bridge::js(&action)
    ));
}

/// The dock: the Mac's own Dock in its order, then the running apps it does not keep, by name —
/// what macOS shows after its separator.
pub fn dock(apps: &[AppEntry]) -> (Vec<&AppEntry>, Vec<&AppEntry>) {
    let mut kept: Vec<&AppEntry> = apps.iter().filter(|a| a.dock.is_some()).collect();
    kept.sort_by_key(|a| a.dock);
    let mut extra: Vec<&AppEntry> = apps
        .iter()
        .filter(|a| a.dock.is_none() && a.running)
        .collect();
    extra.sort_by_key(|a| a.name.to_lowercase());
    (kept, extra)
}

fn named<'a>(apps: &'a [AppEntry], query: &str) -> impl Iterator<Item = &'a AppEntry> {
    let q = query.trim().to_lowercase();
    apps.iter()
        .filter(move |a| !a.hidden && a.name.to_lowercase().contains(&q))
}

/// Open now: the running apps whose name has `query` — the one in front, then the Dock's order,
/// then by name.
pub fn open_now<'a>(apps: &'a [AppEntry], query: &str) -> Vec<&'a AppEntry> {
    let mut v: Vec<&AppEntry> = named(apps, query).filter(|a| a.running).collect();
    v.sort_by_key(|a| (!a.front, a.dock.unwrap_or(u16::MAX), a.name.to_lowercase()));
    v
}

/// Every app whose name has `query`, A–Z.
pub fn a_to_z<'a>(apps: &'a [AppEntry], query: &str) -> Vec<&'a AppEntry> {
    let mut v: Vec<&AppEntry> = named(apps, query).collect();
    v.sort_by_key(|a| a.name.to_lowercase());
    v
}

/// What a tap does: show a hidden app, else open it — which starts it, or brings it to the front
/// (in front already: what a click on the Dock does, a window if it has none).
pub fn tap(a: &AppEntry) -> &'static str {
    if a.running && a.away {
        "unhide"
    } else {
        "open"
    }
}

/// Its state, in words.
pub fn state(a: &AppEntry) -> &'static str {
    match (a.running, a.front, a.away) {
        (false, ..) => "Not open",
        (_, _, true) => "Hidden",
        (_, true, _) => "In front",
        _ => "Running",
    }
}

/// Its menu, as the Dock's: (label, action).
pub fn items(a: &AppEntry) -> Vec<(&'static str, &'static str)> {
    match (a.running, a.away) {
        (false, _) => vec![("Open", "open")],
        (true, true) => vec![
            ("Show", "unhide"),
            ("Quit", "quit"),
            ("Force Quit", "force_quit"),
        ],
        (true, false) => vec![
            ("Show", "open"),
            ("Hide", "hide"),
            ("Quit", "quit"),
            ("Force Quit", "force_quit"),
        ],
    }
}

/// The first letter, for a tile with no icon yet.
pub fn letter(name: &str) -> String {
    name.chars()
        .next()
        .map(|c| c.to_uppercase().to_string())
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn app(name: &str, running: bool, front: bool, away: bool, dock: Option<u16>) -> AppEntry {
        AppEntry {
            name: name.into(),
            exec: format!("/Applications/{name}.app"),
            running,
            front,
            away,
            dock,
            ..Default::default()
        }
    }

    fn names(v: &[&AppEntry]) -> Vec<String> {
        v.iter().map(|a| a.name.clone()).collect()
    }

    fn mac() -> Vec<AppEntry> {
        vec![
            app("Safari", true, false, false, Some(1)),
            app("Finder", true, false, false, Some(0)),
            app("Zed", true, true, false, None),
            app("Notes", false, false, false, Some(2)),
            app("calculator", true, false, true, None),
            app("Chess", false, false, false, None),
        ]
    }

    #[test]
    fn the_dock_is_its_own_order_then_what_else_runs() {
        let apps = mac();
        let (kept, extra) = dock(&apps);
        assert_eq!(names(&kept), ["Finder", "Safari", "Notes"]);
        // Running, not kept, by name whatever the case; Chess is neither.
        assert_eq!(names(&extra), ["calculator", "Zed"]);
    }

    #[test]
    fn open_now_puts_the_front_first_then_the_dock_then_names() {
        let apps = mac();
        assert_eq!(
            names(&open_now(&apps, "")),
            ["Zed", "Finder", "Safari", "calculator"]
        );
        assert_eq!(names(&open_now(&apps, "SAF")), ["Safari"]);
        assert_eq!(
            names(&a_to_z(&apps, "")),
            ["calculator", "Chess", "Finder", "Notes", "Safari", "Zed"]
        );
        assert!(a_to_z(&apps, "photoshop").is_empty());
        assert_eq!(names(&a_to_z(&apps, " c")), ["calculator", "Chess"]);
    }

    #[test]
    fn a_tap_shows_a_hidden_app_and_opens_the_rest() {
        let apps = mac();
        let by = |n: &str| apps.iter().find(|a| a.name == n).unwrap();
        assert_eq!(tap(by("Notes")), "open");
        assert_eq!(tap(by("Safari")), "open");
        assert_eq!(tap(by("calculator")), "unhide");
        assert_eq!(state(by("Notes")), "Not open");
        assert_eq!(state(by("Zed")), "In front");
        assert_eq!(state(by("calculator")), "Hidden");
        assert_eq!(state(by("Safari")), "Running");
    }

    #[test]
    fn the_menu_follows_its_state() {
        let apps = mac();
        let by = |n: &str| items(apps.iter().find(|a| a.name == n).unwrap());
        assert_eq!(by("Notes"), [("Open", "open")]);
        assert_eq!(
            by("Safari"),
            [
                ("Show", "open"),
                ("Hide", "hide"),
                ("Quit", "quit"),
                ("Force Quit", "force_quit")
            ]
        );
        assert_eq!(
            by("calculator"),
            [
                ("Show", "unhide"),
                ("Quit", "quit"),
                ("Force Quit", "force_quit")
            ]
        );
        assert_eq!(letter("éclair"), "É");
    }
}
