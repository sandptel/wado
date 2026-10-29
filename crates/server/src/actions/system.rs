//! The computer itself: lock, sleep, a screenshot, screen brightness.

use std::path::PathBuf;
use std::time::Duration;

use super::{Action, Does, Need};

/// Choosing an area with `slurp` is done by hand on the laptop's screen.
const CHOOSE: Duration = Duration::from_secs(60);
const CAPTURE: Duration = Duration::from_secs(10);

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Lock,
    Suspend,
    Screenshot { region: bool },
    Brightness { up: bool },
}

pub fn all() -> Vec<Action> {
    let a =
        |id, name, icon, needs, op| Action::new(id, name, "System", icon, needs, Does::System(op));
    vec![
        a(
            "lock",
            "Lock screen",
            "lock",
            vec![Need::Tool("loginctl")],
            Op::Lock,
        ),
        a(
            "suspend",
            "Sleep",
            "moon",
            vec![Need::Tool("systemctl")],
            Op::Suspend,
        ),
        a(
            "screenshot",
            "Screenshot",
            "camera",
            vec![Need::Tool("grim"), Need::Desktop],
            Op::Screenshot { region: false },
        ),
        a(
            "screenshot-region",
            "Screenshot of an area",
            "image",
            vec![Need::Tool("grim"), Need::Tool("slurp"), Need::Desktop],
            Op::Screenshot { region: true },
        ),
        a(
            "brightness-up",
            "Brighter",
            "sun",
            vec![Need::Tool("brightnessctl")],
            Op::Brightness { up: true },
        ),
        a(
            "brightness-down",
            "Dimmer",
            "sun",
            vec![Need::Tool("brightnessctl")],
            Op::Brightness { up: false },
        ),
    ]
}

pub async fn run(op: Op) -> Result<(), String> {
    use crate::platform::linux::cmd::run;
    match op {
        Op::Lock => run("loginctl", &["lock-session"]).await.map(drop),
        Op::Suspend => run("systemctl", &["suspend"]).await.map(drop),
        Op::Brightness { up } => {
            let b = crate::platform::get().brightness().map_err(|a| a.why)?;
            b.step(up).await
        }
        Op::Screenshot { region } => screenshot(region).await,
    }
}

async fn screenshot(region: bool) -> Result<(), String> {
    use super::proc::desktop;
    let file = shot_path()?;
    let file = file.to_string_lossy();
    if region {
        let area = desktop("slurp", &[], CHOOSE)
            .await
            .map_err(|_| "no area was chosen".to_string())?;
        desktop("grim", &["-g", &area, &file], CAPTURE).await?;
    } else {
        desktop("grim", &[&file], CAPTURE).await?;
    }
    tracing::info!(file = %file, "actions: screenshot saved");
    Ok(())
}

/// `~/Pictures/wado-2026-10-30_21-04-05.png`, the folder made if missing.
fn shot_path() -> Result<PathBuf, String> {
    let home = std::env::var_os("HOME").ok_or("no home folder to save the screenshot in")?;
    let dir = PathBuf::from(home).join("Pictures");
    std::fs::create_dir_all(&dir).map_err(|e| format!("could not make {}: {e}", dir.display()))?;
    Ok(dir.join(shot_name(crate::files::local(crate::files::now_s()))))
}

fn shot_name(t: libc::tm) -> String {
    format!(
        "wado-{:04}-{:02}-{:02}_{:02}-{:02}-{:02}.png",
        t.tm_year + 1900,
        t.tm_mon + 1,
        t.tm_mday,
        t.tm_hour,
        t.tm_min,
        t.tm_sec
    )
}

#[cfg(test)]
mod tests {
    #[test]
    fn a_screenshot_is_named_by_its_time() {
        // SAFETY: plain data.
        let mut t: libc::tm = unsafe { std::mem::zeroed() };
        (t.tm_year, t.tm_mon, t.tm_mday) = (126, 9, 30);
        (t.tm_hour, t.tm_min, t.tm_sec) = (21, 4, 5);
        assert_eq!(super::shot_name(t), "wado-2026-10-30_21-04-05.png");
    }
}
