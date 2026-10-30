//! The devices a pad drives — the platform's touchpad and keyboard (`wado_platform::input`) —
//! made on first use.
//!
//! The keyboard is made as soon as the pad is, and the touchpad once the phone's size is known,
//! so the desktop has picked both up before the first touch or key. Both go when this does.

use std::sync::Arc;

use wado_platform::Platform;
use wado_platform::input::{InputError, InputRes, KeyInjector, Opener, TouchInjector};
use wado_protocol::pad::{Contact, Mod};

/// The trackpad on a computer wado cannot drive input on yet. `js/pad_why.js` matches it.
pub const UNSUPPORTED: &str = "The trackpad isn't supported on this computer's system yet — wado \
    drives the pointer and keyboard on Linux only, for now.";

pub type OpenTouch = Arc<dyn Opener<dyn TouchInjector>>;
pub type OpenKeys = Arc<dyn Opener<dyn KeyInjector>>;

pub struct Devices {
    /// What makes each device — or why this computer cannot have it.
    open_touch: Result<OpenTouch, InputError>,
    open_keys: Result<OpenKeys, InputError>,
    touch: Option<Box<dyn TouchInjector>>,
    keys: Option<Box<dyn KeyInjector>>,
}

impl Devices {
    pub fn new(open_touch: OpenTouch, open_keys: OpenKeys) -> Devices {
        Devices {
            open_touch: Ok(open_touch),
            open_keys: Ok(open_keys),
            touch: None,
            keys: None,
        }
    }

    /// The platform's devices. A capability that is missing fails every use with why.
    pub fn from_platform(p: &Platform) -> Devices {
        // Off Linux nothing provides input yet: said as that, not as a missing provider.
        let absent = |a: wado_platform::Absent| {
            InputError::Unavailable(if cfg!(target_os = "linux") {
                a.why
            } else {
                UNSUPPORTED.into()
            })
        };
        Devices {
            open_touch: p.touch().map_err(absent),
            open_keys: p.keys().map_err(absent),
            touch: None,
            keys: None,
        }
    }

    fn keyboard(&mut self) -> Result<&mut Box<dyn KeyInjector>, InputError> {
        if self.keys.is_none() {
            let open = self.open_keys.as_ref().map_err(Clone::clone)?;
            self.keys = Some(open.open()?);
        }
        Ok(self.keys.as_mut().expect("just made"))
    }

    /// Make the devices before they are needed — the keyboard always, the touchpad sized when
    /// the size is known (a new size may remake it).
    pub fn ready(&mut self, size: Option<(u16, u16)>) -> InputRes {
        self.keyboard()?;
        let Some((w, h)) = size else {
            return Ok(());
        };
        if self.touch.is_none() {
            let open = self.open_touch.as_ref().map_err(Clone::clone)?;
            self.touch = Some(open.open()?);
        }
        self.touch.as_mut().expect("just made").resize(w, h)
    }

    pub fn touch(&mut self, size: (u16, u16), c: &[Contact]) -> InputRes {
        self.ready(Some(size))?;
        self.touch.as_mut().expect("made by ready").frame(c)
    }

    pub fn tap(&mut self, mods: &[Mod], code: u16) -> InputRes {
        self.keyboard()?.tap(mods, code)
    }

    pub fn hold(&mut self, mods: &[Mod]) -> InputRes {
        self.keyboard()?.hold(mods)
    }

    /// Let go of every held key, if there is a keyboard to hold any.
    pub fn release_all(&mut self) {
        if let Some(k) = self.keys.as_mut() {
            let _ = k.release_all();
        }
    }
}
