//! The keyboard and typed text. Modifiers a phone holds (a locked ⇧ or ⌘) are kept in
//! [`super::HELD`], which pointer events read too, so ⌘-click works; everything else posts its
//! flags explicitly, and dropping the keyboard lets go of everything it held.

use std::sync::atomic::Ordering;

use wado_platform::input::{InputRes, KeyInjector, TextInjector};
use wado_platform::{Fut, Res};
use wado_protocol::pad::Mod;

use super::post::{Poster, flag};
use super::{HELD, keymap};

/// A modifier's key and flag.
fn modkey(m: Mod) -> (u16, u64) {
    match m {
        Mod::Ctrl => (59, flag::CTRL),
        Mod::Alt => (58, flag::ALT),
        Mod::Shift => (56, flag::SHIFT),
        Mod::Super => (55, flag::CMD),
    }
}

fn flags_of(mods: &[Mod]) -> u64 {
    mods.iter().fold(0, |f, m| f | modkey(*m).1)
}

pub struct MacKeys<P: Poster> {
    out: P,
    held: Vec<Mod>,
}

impl<P: Poster> MacKeys<P> {
    pub fn new(out: P) -> Self {
        MacKeys {
            out,
            held: Vec::new(),
        }
    }

    fn publish(&self) {
        HELD.store(flags_of(&self.held), Ordering::Relaxed);
    }
}

impl<P: Poster> KeyInjector for MacKeys<P> {
    fn tap(&mut self, mods: &[Mod], code: u16) -> InputRes {
        let Some(k) = keymap::kvk(code) else {
            return Ok(()); // a key the Mac has no equivalent for: nothing to press
        };
        let extra: Vec<Mod> = mods
            .iter()
            .copied()
            .filter(|m| !self.held.contains(m))
            .collect();
        let mut f = flags_of(&self.held);
        for m in &extra {
            f |= modkey(*m).1;
            self.out.key(modkey(*m).0, true, f);
        }
        let kf = f | keymap::nav_flags(k);
        self.out.key(k, true, kf);
        self.out.key(k, false, kf);
        for m in extra.iter().rev() {
            f &= !modkey(*m).1;
            self.out.key(modkey(*m).0, false, f);
        }
        Ok(())
    }

    fn hold(&mut self, mods: &[Mod]) -> InputRes {
        let mut f = flags_of(&self.held);
        for m in self.held.iter().filter(|m| !mods.contains(m)) {
            f &= !modkey(*m).1;
            self.out.key(modkey(*m).0, false, f);
        }
        for m in mods.iter().filter(|m| !self.held.contains(m)) {
            f |= modkey(*m).1;
            self.out.key(modkey(*m).0, true, f);
        }
        self.held = mods.to_vec();
        self.publish();
        Ok(())
    }
}

impl<P: Poster> Drop for MacKeys<P> {
    fn drop(&mut self) {
        let _ = self.hold(&[]);
    }
}

/// Typed text, whatever the Mac's keyboard layout.
pub struct MacText;

impl TextInjector for MacText {
    fn type_text(&self, text: String) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            super::cg::CgPoster::new().text(&text);
            Ok(())
        })
    }
}

#[cfg(test)]
mod tests {
    use super::super::post::fake::Rec;
    use super::*;

    /// The held flags are one per process: tests that read them take turns.
    static TURN: std::sync::Mutex<()> = std::sync::Mutex::new(());

    #[test]
    fn chord_wraps_the_key_in_its_mods() {
        let mut k = MacKeys::new(Rec::default());
        k.tap(&[Mod::Super], 46).unwrap(); // ⌘C
        assert_eq!(
            k.out.keys(),
            [
                (55, true, flag::CMD),
                (8, true, flag::CMD),
                (8, false, flag::CMD),
                (55, false, 0)
            ]
        );
    }

    #[test]
    fn chord_then_click_has_clean_flags() {
        let _t = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let mut k = MacKeys::new(Rec::default());
        k.tap(&[Mod::Super], 46).unwrap();
        assert_eq!(
            HELD.load(Ordering::Relaxed),
            0,
            "a tapped mod is not left held"
        );
    }

    #[test]
    fn drop_releases_held_mods() {
        let _t = TURN.lock().unwrap_or_else(|e| e.into_inner());
        let mut k = MacKeys::new(Rec::default());
        k.hold(&[Mod::Shift, Mod::Super]).unwrap();
        assert_eq!(HELD.load(Ordering::Relaxed), flag::SHIFT | flag::CMD);
        let mut out = Rec::default();
        std::mem::swap(&mut out, &mut k.out);
        drop(k);
        assert_eq!(HELD.load(Ordering::Relaxed), 0);
    }

    #[test]
    fn arrows_carry_fn_and_keypad_flags() {
        let mut k = MacKeys::new(Rec::default());
        k.tap(&[], 103).unwrap(); // up
        assert_eq!(k.out.keys()[0], (126, true, flag::FN | flag::NUMPAD));
    }
}
