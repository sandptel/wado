//! The screen's backlight, as a level the phone's slider can show and set: `brightnessctl`, on
//! the backlight class only — an LED or a keyboard light is not "the screen".
//!
//! Read on every state poll, which is one short process; absent (`None`) when the computer has
//! no backlight (a desktop with a monitor) or no `brightnessctl`, and the phone then says so
//! instead of offering a slider that does nothing.

use super::cmd::run;

/// 0..1, or `None` when there is nothing to set.
pub async fn level() -> Option<f32> {
    let out = run("brightnessctl", &["--class=backlight", "-m", "info"])
        .await
        .ok()?;
    parse(&out)
}

/// Set the backlight to `level` (0..1). Never fully off: a black panel nobody can see to undo
/// is not a brightness, so the floor is 1%.
pub async fn set(level: f32) -> Result<(), String> {
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
}

/// `brightnessctl -m` prints `device,class,current,percent,max`, one line per device.
fn parse(out: &str) -> Option<f32> {
    let line = out.lines().find(|l| !l.trim().is_empty())?;
    let f: Vec<&str> = line.trim().split(',').collect();
    let (cur, max) = (
        f.get(2)?.parse::<f32>().ok()?,
        f.get(4)?.parse::<f32>().ok()?,
    );
    (max > 0.0).then(|| (cur / max).clamp(0.0, 1.0))
}

#[cfg(test)]
mod tests {
    use super::parse;

    #[test]
    fn reads_the_machine_line() {
        assert_eq!(
            parse("intel_backlight,backlight,12000,50%,24000\n"),
            Some(0.5)
        );
        assert_eq!(parse("amdgpu_bl0,backlight,255,100%,255"), Some(1.0));
    }

    #[test]
    fn nothing_to_read_is_none() {
        assert_eq!(parse(""), None);
        assert_eq!(parse("garbage"), None);
        assert_eq!(parse("x,backlight,5,0%,0"), None);
    }
}
