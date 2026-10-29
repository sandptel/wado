//! The default output's volume and mute, through the platform's mixer (`wado_platform::audio`).

use super::{Action, Does, Need};

/// A volume step: one tap of a laptop's volume key.
const STEP: f32 = 0.05;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Op {
    Up,
    Down,
    Mute,
}

pub fn all() -> Vec<Action> {
    [
        ("volume-up", "Volume up", "sound", Op::Up),
        ("volume-down", "Volume down", "sound", Op::Down),
        ("mute", "Mute", "mute", Op::Mute),
    ]
    .into_iter()
    .map(|(id, name, icon, op)| {
        Action::new(
            id,
            name,
            "Sound",
            icon,
            vec![Need::Tool("wpctl"), Need::Tool("pw-dump")],
            Does::Sound(op),
        )
    })
    .collect()
}

pub async fn run(op: Op) -> Result<(), String> {
    let p = crate::platform::get();
    let mixer = p.audio().map_err(|a| a.why)?;
    let a = crate::host::audio::now(mixer.as_ref(), crate::host::viewer::name(p).as_deref()).await;
    let out = a
        .sinks
        .iter()
        .find(|s| s.name == a.default_sink)
        .ok_or("this computer has no sound output")?;
    match op {
        Op::Up => mixer.set_volume(out.id, step(out.volume, STEP)).await,
        Op::Down => mixer.set_volume(out.id, step(out.volume, -STEP)).await,
        Op::Mute => mixer.set_mute(out.id, !out.muted).await,
    }
}

/// One step from `v`, held to 0..=100 % — but a volume someone set above 100 % is not pulled
/// down by pressing "up".
fn step(v: f32, by: f32) -> f32 {
    let to = (v + by).clamp(0.0, 1.0);
    if by > 0.0 { to.max(v) } else { to }
}

#[cfg(test)]
mod tests {
    #[test]
    fn steps_stay_in_range() {
        assert!((super::step(0.5, 0.05) - 0.55).abs() < 1e-6);
        assert_eq!(super::step(0.98, 0.05), 1.0);
        assert_eq!(super::step(1.2, 0.05), 1.2);
        assert_eq!(super::step(0.02, -0.05), 0.0);
    }
}
