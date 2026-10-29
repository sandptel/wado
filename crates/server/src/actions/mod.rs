//! Actions — things a phone can make this computer do with one tap: next workspace, fullscreen,
//! play/pause, volume, lock. The trackpad's rail and gestures run them by id (`pad::actions`).
//!
//! This file is the registry: every action, which of them this computer can run, and running
//! one. An action whose tool or desktop is missing is never listed — a button that silently does
//! nothing is worse than no button — and running one anyway says why in plain words.
//!
//! Files, one job each:
//! - `detect.rs` — what this computer has: Hyprland, the tools, a session bus. Cached.
//! - `hypr.rs` — Hyprland's instance signature, and `hyprctl dispatch`.
//! - `proc.rs` — running a tool: awaited with a deadline, or spawned detached.
//! - `window.rs`, `workspace.rs`, `media.rs`, `sound.rs`, `system.rs`, `apps.rs` — the actions
//!   of each group, and how each runs.
//!
//! Commands are argument lists, never a shell string.

pub mod apps;
pub mod detect;
pub mod hypr;
pub mod media;
pub mod proc;
pub mod sound;
pub mod system;
pub mod window;
pub mod workspace;

use wado_protocol::pad::ActionInfo;

pub use detect::{Detect, Need};

/// One action: what the phone is shown, what it needs here, and what it does.
#[derive(Debug, Clone, PartialEq)]
pub struct Action {
    pub info: ActionInfo,
    /// Every one must be present for the action to be listed.
    pub needs: Vec<Need>,
    pub does: Does,
}

/// How an action runs — each group's own file carries it out.
#[derive(Debug, Clone, PartialEq)]
pub enum Does {
    /// `hyprctl` with these arguments (`["dispatch", "workspace", "e+1"]`).
    Hypr(Vec<String>),
    Media(wado_protocol::host::MediaOp),
    Sound(sound::Op),
    System(system::Op),
    App(apps::Op),
}

impl Action {
    pub fn new(
        id: &str,
        name: &str,
        group: &str,
        icon: &str,
        needs: Vec<Need>,
        does: Does,
    ) -> Self {
        Self {
            info: ActionInfo {
                id: id.into(),
                name: name.into(),
                group: group.into(),
                icon: icon.into(),
            },
            needs,
            does,
        }
    }

    /// The first thing it needs that `found` lacks.
    fn missing(&self, found: &impl Detect) -> Option<&Need> {
        self.needs.iter().find(|n| !found.has(n))
    }
}

/// Every action wado knows, available here or not, in the order the phone shows them.
pub fn all() -> Vec<Action> {
    [
        window::all(),
        workspace::all(),
        media::all(),
        sound::all(),
        system::all(),
        apps::all(),
    ]
    .concat()
}

/// The actions `found` can run.
pub fn available(found: &impl Detect) -> Vec<ActionInfo> {
    all()
        .into_iter()
        .filter(|a| a.missing(found).is_none())
        .map(|a| a.info)
        .collect()
}

/// The actions this computer can run now.
pub async fn list() -> Vec<ActionInfo> {
    available(&detect::now().await)
}

/// Run the action `id`; why not, in plain words, when it did not.
pub async fn run(id: &str) -> Result<(), String> {
    let action = find(id)?;
    let mut found = detect::now().await;
    if action.missing(&found).is_some() {
        // A miss: the cache may predate a tool just installed or a Hyprland just restarted.
        detect::forget();
        found = detect::now().await;
    }
    if let Some(need) = action.missing(&found) {
        return Err(format!("{} — {}", action.info.name, need.why()));
    }
    let done = match action.does {
        Does::Hypr(args) => hypr::dispatch(&args).await,
        Does::Media(op) => media::run(op).await,
        Does::Sound(op) => sound::run(op).await,
        Does::System(op) => system::run(op).await,
        Does::App(op) => apps::run(op, &found),
    };
    if done.is_err() {
        detect::forget();
    }
    done
}

/// The action `id`, or the error the phone is shown for an id this daemon does not know.
pub fn find(id: &str) -> Result<Action, String> {
    all()
        .into_iter()
        .find(|a| a.info.id == id)
        .ok_or_else(|| format!("unknown action \"{id}\""))
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A computer with exactly these.
    struct Fake(Vec<Need>);
    impl Detect for Fake {
        fn has(&self, need: &Need) -> bool {
            self.0.contains(need)
        }
    }

    fn ids(found: &impl Detect) -> Vec<String> {
        available(found).into_iter().map(|a| a.id).collect()
    }

    #[test]
    fn lists_only_what_this_computer_has() {
        assert!(
            ids(&Fake(vec![])).is_empty(),
            "nothing here, nothing listed"
        );

        let hypr = ids(&Fake(vec![Need::Hyprland]));
        assert!(hypr.contains(&"workspace-next".to_string()));
        assert!(hypr.contains(&"fullscreen".to_string()));
        assert!(!hypr.contains(&"play-pause".to_string()));
        assert!(
            available(&Fake(vec![Need::Hyprland]))
                .iter()
                .all(|a| a.group == "Window" || a.group == "Workspace")
        );

        let media = ids(&Fake(vec![Need::Mpris]));
        assert_eq!(media, ["play-pause", "next-track", "prev-track"]);

        // A screenshot needs the tool and the desktop to aim it at.
        let grim = ids(&Fake(vec![Need::Tool("grim")]));
        assert!(!grim.contains(&"screenshot".to_string()));
        let grim = ids(&Fake(vec![Need::Tool("grim"), Need::Desktop]));
        assert!(grim.contains(&"screenshot".to_string()));
        assert!(!grim.contains(&"screenshot-region".to_string()), "no slurp");
    }

    #[test]
    fn ids_are_unique_and_described() {
        let all = all();
        let mut ids: Vec<_> = all.iter().map(|a| a.info.id.as_str()).collect();
        ids.sort();
        ids.dedup();
        assert_eq!(ids.len(), all.len());
        for a in &all {
            assert!(!a.info.name.is_empty() && !a.info.icon.is_empty(), "{a:?}");
            assert!(
                ["Window", "Workspace", "Media", "Sound", "System", "Apps"]
                    .contains(&a.info.group.as_str()),
                "{a:?}"
            );
            assert!(!a.needs.is_empty(), "{} must say what it needs", a.info.id);
        }
    }

    #[test]
    fn workspace_next_is_exactly_hyprctl_dispatch() {
        assert_eq!(
            find("workspace-next").unwrap().does,
            Does::Hypr(vec!["dispatch".into(), "workspace".into(), "e+1".into()])
        );
        assert_eq!(
            find("move-to-workspace-3").unwrap().does,
            Does::Hypr(vec![
                "dispatch".into(),
                "movetoworkspace".into(),
                "3".into()
            ])
        );
        assert_eq!(
            find("focus-left").unwrap().does,
            Does::Hypr(vec!["dispatch".into(), "movefocus".into(), "l".into()])
        );
    }

    #[tokio::test]
    async fn an_unknown_id_is_refused_by_name() {
        let e = run("make-coffee").await.unwrap_err();
        assert!(e.starts_with("unknown action"), "{e}");
        assert!(e.contains("make-coffee"), "{e}");
    }

    /// Live, on the user's Hyprland laptop: `cargo test -p wado actions::tests::live -- --ignored`
    /// with the sandbox off.
    #[tokio::test]
    #[ignore]
    async fn live_lists_the_window_group() {
        hypr::capture();
        crate::clip::env::capture();
        let list = list().await;
        let ids: Vec<_> = list.iter().map(|a| a.id.as_str()).collect();
        eprintln!("{} actions here: {}", ids.len(), ids.join(" "));
        assert!(list.iter().any(|a| a.group == "Window"), "{list:?}");
    }

    /// Live: next then previous workspace leaves the desktop where it was.
    #[tokio::test]
    #[ignore]
    async fn live_workspace_next_then_prev_returns() {
        hypr::capture();
        let before = hypr::active_workspace().await.unwrap();
        run("workspace-next").await.unwrap();
        run("workspace-prev").await.unwrap();
        let after = hypr::active_workspace().await.unwrap();
        assert_eq!(before, after);
    }
}
