//! Fake input devices: they record every call, in order, in one shared [`InputLog`], so a test
//! can assert exactly what reached "the device" across touch and keys together.

use std::sync::{Arc, Mutex};

use wado_protocol::pad::{Contact, Mod};

use crate::input::{
    GamepadEvent, GamepadInjector, InputError, InputRes, KeyInjector, Opener, TextInjector,
    TouchInjector,
};
use crate::{Fut, Res};

/// One call that reached a fake device.
#[derive(Debug, Clone, PartialEq)]
pub enum InputCall {
    /// A touch device was opened.
    OpenTouch,
    /// A keyboard device was opened.
    OpenKeys,
    /// A gamepad device was opened.
    OpenGamepad,
    /// A touch frame, with the size the pad had at the time.
    Touch((u16, u16), Vec<Contact>),
    Tap(Vec<Mod>, u16),
    Hold(Vec<Mod>),
    Release,
    Gamepad(GamepadEvent),
    Text(String),
}

/// What the fake devices did, in order. Clone it into the test before handing the openers over.
#[derive(Clone, Default)]
pub struct InputLog {
    calls: Arc<Mutex<Vec<InputCall>>>,
    /// When set, opening any device fails with this.
    refuse: Arc<Mutex<Option<InputError>>>,
}

impl InputLog {
    pub fn calls(&self) -> Vec<InputCall> {
        self.calls.lock().unwrap().clone()
    }

    /// The calls, without the device openings.
    pub fn io(&self) -> Vec<InputCall> {
        self.calls()
            .into_iter()
            .filter(|c| {
                !matches!(
                    c,
                    InputCall::OpenTouch | InputCall::OpenKeys | InputCall::OpenGamepad
                )
            })
            .collect()
    }

    /// Every later open fails with `e` — no permission, no driver.
    pub fn refuse(&self, e: InputError) {
        *self.refuse.lock().unwrap() = Some(e);
    }

    fn push(&self, c: InputCall) {
        self.calls.lock().unwrap().push(c);
    }

    fn opened(&self, c: InputCall) -> Result<(), InputError> {
        if let Some(e) = self.refuse.lock().unwrap().clone() {
            return Err(e);
        }
        self.push(c);
        Ok(())
    }

    /// An opener of fake touchpads that record here.
    pub fn touch(&self) -> Arc<dyn Opener<dyn TouchInjector>> {
        let log = self.clone();
        Arc::new(move || {
            log.opened(InputCall::OpenTouch)?;
            Ok(Box::new(FakeTouch {
                log: log.clone(),
                size: None,
            }) as Box<dyn TouchInjector>)
        })
    }

    /// An opener of fake keyboards that record here.
    pub fn keys(&self) -> Arc<dyn Opener<dyn KeyInjector>> {
        let log = self.clone();
        Arc::new(move || {
            log.opened(InputCall::OpenKeys)?;
            Ok(Box::new(FakeKeys { log: log.clone() }) as Box<dyn KeyInjector>)
        })
    }

    /// An opener of fake gamepads that record here.
    pub fn gamepad(&self) -> Arc<dyn Opener<dyn GamepadInjector>> {
        let log = self.clone();
        Arc::new(move || {
            log.opened(InputCall::OpenGamepad)?;
            Ok(Box::new(FakeGamepad { log: log.clone() }) as Box<dyn GamepadInjector>)
        })
    }

    /// A fake typist that records here.
    pub fn text(&self) -> Arc<dyn TextInjector> {
        Arc::new(FakeText { log: self.clone() })
    }
}

pub struct FakeTouch {
    log: InputLog,
    size: Option<(u16, u16)>,
}

impl TouchInjector for FakeTouch {
    fn resize(&mut self, w: u16, h: u16) -> InputRes {
        self.size = Some((w, h));
        Ok(())
    }
    fn frame(&mut self, contacts: &[Contact]) -> InputRes {
        let size = self
            .size
            .ok_or_else(|| InputError::Failed("a frame before the size".into()))?;
        self.log.push(InputCall::Touch(size, contacts.to_vec()));
        Ok(())
    }
}

pub struct FakeKeys {
    log: InputLog,
}

impl KeyInjector for FakeKeys {
    fn tap(&mut self, mods: &[Mod], code: u16) -> InputRes {
        self.log.push(InputCall::Tap(mods.to_vec(), code));
        Ok(())
    }
    fn hold(&mut self, mods: &[Mod]) -> InputRes {
        self.log.push(InputCall::Hold(mods.to_vec()));
        Ok(())
    }
    fn release_all(&mut self) -> InputRes {
        self.log.push(InputCall::Release);
        Ok(())
    }
}

pub struct FakeGamepad {
    log: InputLog,
}

impl GamepadInjector for FakeGamepad {
    fn report(&mut self, ev: GamepadEvent) {
        self.log.push(InputCall::Gamepad(ev));
    }
}

pub struct FakeText {
    log: InputLog,
}

impl TextInjector for FakeText {
    fn type_text(&self, text: String) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            self.log.push(InputCall::Text(text));
            Ok(())
        })
    }
}
