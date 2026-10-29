//! The screen's backlight ([`Brightness`] on Linux), as a level the phone's slider can show and
//! set: `brightnessctl`, on the backlight class only — an LED or a keyboard light is not "the
//! screen".
//!
//! `brightnessctl` says which device is the screen once; after that the level is read straight
//! from that device's `brightness` / `max_brightness` in sysfs — two small file reads instead of
//! a process on every refresh. Absent (`None`) when the computer has no backlight (a desktop with
//! a monitor), and the phone then says so instead of offering a slider that does nothing.

use std::path::{Path, PathBuf};
use std::sync::Mutex;
use std::time::Duration;

use tokio::sync::watch;
use wado_platform::display::Brightness;
use wado_platform::{Fut, Published, Res};

use super::cmd::run;

/// Where the kernel lists backlights.
const SYSFS: &str = "/sys/class/backlight";

/// The backlight, through `brightnessctl` and sysfs.
pub struct Backlight {
    state: Published<Option<f32>>,
    /// The device `brightnessctl` named, once it has.
    device: Mutex<Option<String>>,
    sysfs: PathBuf,
}

impl Backlight {
    pub fn new() -> Backlight {
        Backlight {
            state: Published::new(None),
            device: Mutex::new(None),
            sysfs: PathBuf::from(SYSFS),
        }
    }

    /// 0..1, or `None` when there is nothing to set.
    async fn read(&self) -> Option<f32> {
        let known = self
            .device
            .lock()
            .unwrap_or_else(|e| e.into_inner())
            .clone();
        if let Some(level) = known.and_then(|d| sysfs_level(&self.sysfs.join(d))) {
            return Some(level);
        }
        let out = run("brightnessctl", &["--class=backlight", "-m", "info"])
            .await
            .ok()?;
        let (device, level) = parse(&out)?;
        *self.device.lock().unwrap_or_else(|e| e.into_inner()) = Some(device);
        Some(level)
    }
}

impl Brightness for Backlight {
    fn state(&self) -> watch::Receiver<Option<f32>> {
        self.state.subscribe()
    }

    fn fresh(&self, within: Duration) -> Fut<'_, ()> {
        Box::pin(self.state.fresh(within, || self.read()))
    }

    /// Never fully off: a black panel nobody can see to undo is not a brightness, so the floor
    /// is 1%.
    fn set(&self, level: f32) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            if !level.is_finite() {
                return Err("that is not a brightness".into());
            }
            let pct = (level.clamp(0.0, 1.0) * 100.0).round().max(1.0) as u32;
            run(
                "brightnessctl",
                &["--class=backlight", "set", &format!("{pct}%")],
            )
            .await
            .map(drop)
        })
    }

    /// 10% up or down, on `brightnessctl`'s own default device — what a laptop's keys do.
    fn step(&self, up: bool) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            run("brightnessctl", &["set", if up { "10%+" } else { "10%-" }])
                .await
                .map(drop)
        })
    }
}

/// The level of the backlight in `dir`, from its sysfs files.
fn sysfs_level(dir: &Path) -> Option<f32> {
    let num = |f: &str| -> Option<f32> {
        std::fs::read_to_string(dir.join(f))
            .ok()?
            .trim()
            .parse()
            .ok()
    };
    let (cur, max) = (num("brightness")?, num("max_brightness")?);
    (max > 0.0).then(|| (cur / max).clamp(0.0, 1.0))
}

/// `brightnessctl -m` prints `device,class,current,percent,max`, one line per device: the first
/// one's name and level.
fn parse(out: &str) -> Option<(String, f32)> {
    let line = out.lines().find(|l| !l.trim().is_empty())?;
    let f: Vec<&str> = line.trim().split(',').collect();
    let (cur, max) = (
        f.get(2)?.parse::<f32>().ok()?,
        f.get(4)?.parse::<f32>().ok()?,
    );
    (max > 0.0).then(|| (f[0].to_string(), (cur / max).clamp(0.0, 1.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reads_the_machine_line() {
        assert_eq!(
            parse("intel_backlight,backlight,12000,50%,24000\n"),
            Some(("intel_backlight".into(), 0.5))
        );
        assert_eq!(
            parse("amdgpu_bl0,backlight,255,100%,255"),
            Some(("amdgpu_bl0".into(), 1.0))
        );
    }

    #[test]
    fn nothing_to_read_is_none() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("garbage"), None);
        assert_eq!(parse("x,backlight,5,0%,0"), None);
    }

    /// Once the device is known, the level comes from sysfs with no process at all.
    #[tokio::test]
    async fn a_known_device_is_read_from_sysfs() {
        let dir = std::env::temp_dir().join(format!("wado-bl-{}", rand::random::<u64>()));
        std::fs::create_dir_all(dir.join("panel")).unwrap();
        std::fs::write(dir.join("panel/brightness"), "300\n").unwrap();
        std::fs::write(dir.join("panel/max_brightness"), "1200\n").unwrap();
        let b = Backlight {
            state: Published::new(None),
            device: Mutex::new(Some("panel".into())),
            sysfs: dir.clone(),
        };
        assert_eq!(
            b.read().await,
            Some(0.25),
            "1200 is not what brightnessctl says"
        );
        std::fs::remove_dir_all(&dir).ok();
    }
}
