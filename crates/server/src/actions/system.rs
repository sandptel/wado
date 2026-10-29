//! The computer itself: lock, sleep, a screenshot, screen brightness — through the platform's
//! system power, screenshots and backlight. Where a screenshot is saved is decided here.

use std::path::PathBuf;

use wado_platform::power::PowerOp;
use wado_platform::{Capability, Platform};

use super::{Action, Does, Need};

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
    let shot = Need::Cap(Capability::Screenshot);
    let backlight = Need::Cap(Capability::Brightness);
    vec![
        a(
            "lock",
            "Lock screen",
            "lock",
            vec![Need::Power(PowerOp::Lock)],
            Op::Lock,
        ),
        a(
            "suspend",
            "Sleep",
            "moon",
            vec![Need::Power(PowerOp::Suspend)],
            Op::Suspend,
        ),
        a(
            "screenshot",
            "Screenshot",
            "camera",
            vec![shot],
            Op::Screenshot { region: false },
        ),
        a(
            "screenshot-region",
            "Screenshot of an area",
            "image",
            vec![shot, Need::Region],
            Op::Screenshot { region: true },
        ),
        a(
            "brightness-up",
            "Brighter",
            "sun",
            vec![backlight],
            Op::Brightness { up: true },
        ),
        a(
            "brightness-down",
            "Dimmer",
            "sun",
            vec![backlight],
            Op::Brightness { up: false },
        ),
    ]
}

pub async fn run(p: &Platform, op: Op) -> Result<(), String> {
    let absent = |a: wado_platform::Absent| a.why;
    match op {
        Op::Lock => p.system_power().map_err(absent)?.run(PowerOp::Lock).await,
        Op::Suspend => {
            p.system_power()
                .map_err(absent)?
                .run(PowerOp::Suspend)
                .await
        }
        Op::Brightness { up } => p.brightness().map_err(absent)?.step(up).await,
        Op::Screenshot { region } => {
            let file = shot_path()?;
            p.screenshot()
                .map_err(absent)?
                .shoot(region, file.clone())
                .await?;
            tracing::info!(file = %file.display(), "actions: screenshot saved");
            Ok(())
        }
    }
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
