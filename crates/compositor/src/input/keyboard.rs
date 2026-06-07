//! Keyboard input synthesis: [`wado_protocol::InputEvent::Key`] → `wl_keyboard`.
//!
//! `code` is a Linux evdev keycode; xkb wants evdev+8 (the broken-X keycode base), which
//! is exactly what Smithay's libinput backend applies.

use smithay::{
    backend::input::KeyState,
    input::keyboard::{FilterResult, Keycode},
};

use crate::Wado;

impl Wado {
    /// Synthesize a key press/release on the seat keyboard.
    pub(crate) fn key(&mut self, code: u32, pressed: bool) {
        let (serial, time) = self.input_clock();
        let state = if pressed { KeyState::Pressed } else { KeyState::Released };
        let keyboard = self.seat.get_keyboard().unwrap();
        keyboard.input::<(), _>(
            self,
            Keycode::new(code + 8),
            state,
            serial,
            time,
            |_, _, _| FilterResult::Forward,
        );
    }
}
