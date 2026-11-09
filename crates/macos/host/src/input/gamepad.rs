//! The phone's game controller as keys: macOS has no virtual controller without a driver Apple
//! must approve, so the d-pad and left stick are the arrow keys, A Return, B Esc, X Space,
//! Y Tab, the bumpers Page Up/Down, Back ⌫ and Start Return — enough for menus, emulators that
//! read the keyboard, and most games' keyboard controls.

use std::collections::BTreeSet;

use wado_platform::input::{GamepadEvent, GamepadInjector};

use super::post::Poster;

/// Stick travel (of ±32767) past which it is an arrow.
pub const STICK: i32 = 16_000;

const UP: u16 = 126;
const DOWN: u16 = 125;
const LEFT: u16 = 123;
const RIGHT: u16 = 124;

fn button_key(code: u16) -> Option<u16> {
    Some(match code {
        0x130 => 36,  // A → Return
        0x131 => 53,  // B → Escape
        0x133 => 49,  // X → Space
        0x134 => 48,  // Y → Tab
        0x136 => 116, // LB → Page Up
        0x137 => 121, // RB → Page Down
        0x13a => 51,  // Back → Delete
        0x13b => 36,  // Start → Return
        _ => return None,
    })
}

pub struct PadKeys<P: Poster> {
    out: P,
    /// Keys down now, and the arrows each source wants.
    down: BTreeSet<u16>,
    hat: (i32, i32),
    stick: (i32, i32),
}

impl<P: Poster> PadKeys<P> {
    pub fn new(out: P) -> Self {
        PadKeys {
            out,
            down: BTreeSet::new(),
            hat: (0, 0),
            stick: (0, 0),
        }
    }

    fn set(&mut self, k: u16, on: bool) {
        let f = super::keymap::nav_flags(k);
        if on && self.down.insert(k) {
            self.out.key(k, true, f);
        } else if !on && self.down.remove(&k) {
            self.out.key(k, false, f);
        }
    }

    fn arrows(&mut self) {
        let x = if self.hat.0 != 0 {
            self.hat.0
        } else {
            self.stick.0
        };
        let y = if self.hat.1 != 0 {
            self.hat.1
        } else {
            self.stick.1
        };
        self.set(LEFT, x < 0);
        self.set(RIGHT, x > 0);
        self.set(UP, y < 0);
        self.set(DOWN, y > 0);
    }
}

impl<P: Poster> GamepadInjector for PadKeys<P> {
    fn report(&mut self, ev: GamepadEvent) {
        let dir = |v: i32| {
            if v <= -STICK {
                -1
            } else if v >= STICK {
                1
            } else {
                0
            }
        };
        match ev {
            GamepadEvent::Button { code, pressed } => {
                if let Some(k) = button_key(code) {
                    self.set(k, pressed);
                }
            }
            GamepadEvent::Axis { code: 0x10, value } => {
                self.hat.0 = value.signum();
                self.arrows();
            }
            GamepadEvent::Axis { code: 0x11, value } => {
                self.hat.1 = value.signum();
                self.arrows();
            }
            GamepadEvent::Axis { code: 0x00, value } => {
                self.stick.0 = dir(value);
                self.arrows();
            }
            GamepadEvent::Axis { code: 0x01, value } => {
                self.stick.1 = dir(value);
                self.arrows();
            }
            GamepadEvent::Axis { .. } => {}
        }
    }
}

impl<P: Poster> Drop for PadKeys<P> {
    fn drop(&mut self) {
        for k in std::mem::take(&mut self.down) {
            self.out.key(k, false, super::keymap::nav_flags(k));
        }
    }
}

#[cfg(test)]
mod tests {
    use super::super::post::fake::Rec;
    use super::*;

    #[test]
    fn a_is_return_and_released() {
        let mut p = PadKeys::new(Rec::default());
        p.report(GamepadEvent::Button {
            code: 0x130,
            pressed: true,
        });
        p.report(GamepadEvent::Button {
            code: 0x130,
            pressed: false,
        });
        assert_eq!(p.out.keys(), [(36, true, 0), (36, false, 0)]);
    }

    #[test]
    fn stick_past_threshold_is_an_arrow_once() {
        let mut p = PadKeys::new(Rec::default());
        p.report(GamepadEvent::Axis {
            code: 0x00,
            value: -20_000,
        });
        p.report(GamepadEvent::Axis {
            code: 0x00,
            value: -30_000,
        });
        p.report(GamepadEvent::Axis {
            code: 0x00,
            value: 0,
        });
        let k: Vec<_> = p.out.keys().iter().map(|k| (k.0, k.1)).collect();
        assert_eq!(k, [(LEFT, true), (LEFT, false)]);
    }

    #[test]
    fn nothing_stays_down_when_dropped() {
        let mut p = PadKeys::new(Rec::default());
        p.report(GamepadEvent::Axis {
            code: 0x11,
            value: 1,
        });
        assert!(p.down.contains(&DOWN));
        let held = p.down.clone();
        drop(p);
        assert_eq!(held.len(), 1);
    }
}
