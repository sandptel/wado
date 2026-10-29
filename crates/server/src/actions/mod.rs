//! Actions — things a phone can make this computer do with one tap: next workspace, fullscreen,
//! play/pause, volume, lock. The trackpad's rail and gestures run them by id (`pad::actions`).
//!
//! This file is the registry: every action, which of them this computer can run, and running
//! one. An action whose capability is missing is never listed — a button that silently does
//! nothing is worse than no button — and running one anyway says why in plain words.
//!
//! What an action does is the platform's (`wado_platform`: the window manager, the launcher,
//! screenshots, system power, the mixer, media players, the backlight); what it needs is said as
//! capabilities, so a platform without one simply lists fewer actions.
//!
//! Files, one job each:
//! - `detect.rs` — what an action needs, as capabilities, and asking the platform.
//! - `window.rs`, `workspace.rs`, `media.rs`, `sound.rs`, `system.rs`, `apps.rs` — the actions
//!   of each group, and how each runs.

pub mod apps;
pub mod detect;
pub mod media;
pub mod sound;
pub mod system;
pub mod window;
pub mod workspace;

use wado_platform::desktop::{App, WindowOp};
use wado_platform::{Capability, Platform};
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
    /// The window manager does this.
    Window(WindowOp),
    Media(wado_protocol::host::MediaOp),
    Sound(sound::Op),
    System(system::Op),
    App(App),
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

    /// Why `found` cannot run it: what the first missing need says.
    fn missing(&self, found: &impl Detect) -> Option<String> {
        self.needs.iter().find_map(|n| found.check(n).err())
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

/// Every capability some action needs.
pub fn used() -> Vec<Capability> {
    let mut caps: Vec<Capability> = all()
        .iter()
        .flat_map(|a| a.needs.iter().map(Need::cap))
        .collect();
    caps.sort_unstable();
    caps.dedup();
    caps
}

/// The actions this computer can run now.
pub async fn list() -> Vec<ActionInfo> {
    let p = crate::platform::get();
    detect::fresh(p).await;
    available(p)
}

/// Run the action `id`; why not, in plain words, when it did not.
pub async fn run(id: &str) -> Result<(), String> {
    let action = find(id)?;
    let p = crate::platform::get();
    detect::fresh(p).await;
    if action.missing(p).is_some() {
        // A miss: the probe may predate a tool just installed or a Hyprland just restarted.
        detect::look_again(p, action.needs.iter().map(Need::cap).collect()).await;
    }
    if let Some(why) = action.missing(p) {
        return Err(format!("{} — {}", action.info.name, why));
    }
    let done = does(p, action.does).await;
    if done.is_err() {
        detect::forget();
    }
    done
}

/// Carry out `does` through the platform.
async fn does(p: &Platform, does: Does) -> Result<(), String> {
    let absent = |a: wado_platform::Absent| a.why;
    match does {
        Does::Window(op) => p.window_manager().map_err(absent)?.run(op).await,
        Does::Media(op) => media::run(p, op).await,
        Does::Sound(op) => sound::run(p, op).await,
        Does::System(op) => system::run(p, op).await,
        Does::App(app) => p.launcher().map_err(absent)?.open(app),
    }
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
        fn check(&self, need: &Need) -> Result<(), String> {
            self.0
                .contains(need)
                .then_some(())
                .ok_or_else(|| format!("no {need:?}"))
        }
    }

    use wado_platform::desktop::Dir;
    use wado_platform::power::PowerOp;
    const HYPRLAND: Need = Need::Cap(Capability::WindowManager);

    fn ids(found: &impl Detect) -> Vec<String> {
        available(found).into_iter().map(|a| a.id).collect()
    }

    #[test]
    fn lists_only_what_this_computer_has() {
        assert!(
            ids(&Fake(vec![])).is_empty(),
            "nothing here, nothing listed"
        );

        let hypr = ids(&Fake(vec![HYPRLAND]));
        assert!(hypr.contains(&"workspace-next".to_string()));
        assert!(hypr.contains(&"fullscreen".to_string()));
        assert!(!hypr.contains(&"play-pause".to_string()));
        assert!(
            available(&Fake(vec![HYPRLAND]))
                .iter()
                .all(|a| a.group == "Window" || a.group == "Workspace")
        );

        let media = ids(&Fake(vec![Need::Cap(Capability::Media)]));
        assert_eq!(media, ["play-pause", "next-track", "prev-track"]);

        // A screenshot of an area needs the person to be able to choose one.
        let shot = ids(&Fake(vec![Need::Cap(Capability::Screenshot)]));
        assert!(shot.contains(&"screenshot".to_string()));
        assert!(!shot.contains(&"screenshot-region".to_string()), "no area");
        let shot = ids(&Fake(vec![Need::Cap(Capability::Screenshot), Need::Region]));
        assert!(shot.contains(&"screenshot-region".to_string()));

        // Lock and sleep are each their own operation.
        let lock = ids(&Fake(vec![Need::Power(PowerOp::Lock)]));
        assert_eq!(lock, ["lock"]);
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
    fn window_actions_are_window_ops() {
        assert_eq!(
            find("workspace-next").unwrap().does,
            Does::Window(WindowOp::WorkspaceStep(Dir::Right))
        );
        assert_eq!(
            find("move-to-workspace-3").unwrap().does,
            Does::Window(WindowOp::MoveToWorkspace(3))
        );
        assert_eq!(
            find("focus-left").unwrap().does,
            Does::Window(WindowOp::Focus(Dir::Left))
        );
    }

    /// Run through a platform of fakes: each action reaches its capability.
    #[tokio::test]
    async fn actions_run_through_the_platform() {
        let f = wado_platform::fake::Fakes::new();
        let p = f.platform();
        for id in ["workspace-next", "lock", "open-files", "brightness-up"] {
            does(&p, find(id).unwrap().does).await.unwrap();
        }
        assert_eq!(
            f.log.calls(),
            [
                "window.run WorkspaceStep(Right)",
                "power.run Lock",
                "launch.open Files",
                "brightness.step true"
            ]
        );
    }

    /// A missing capability is named, in the words the platform gave.
    #[test]
    fn a_missing_capability_says_why() {
        let p = wado_platform::PlatformBuilder::empty()
            .with(wado_platform::fake::Fakes::new().provider())
            .without(Capability::WindowManager)
            .probe();
        let a = find("fullscreen").unwrap();
        assert_eq!(
            a.missing(&p),
            Some(wado_platform::registry::LEFT_OUT.to_string())
        );
        assert_eq!(find("lock").unwrap().missing(&p), None);
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
        crate::platform::linux::hyprland::capture();
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
        use crate::platform::linux::hyprland;
        hyprland::capture();
        let before = hyprland::active_workspace().await.unwrap();
        run("workspace-next").await.unwrap();
        run("workspace-prev").await.unwrap();
        let after = hyprland::active_workspace().await.unwrap();
        assert_eq!(before, after);
    }
}
