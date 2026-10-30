//! Where the person is, outside a session: home, one computer's page, or a settings page pushed
//! over either. And the moves between them.
//!
//! A computer is named by its profile key ([`crate::profile::Profile::key`]), never by its
//! index, so a page stays on the same computer while the list changes under it. Opening a
//! computer's page selects that computer — dials it — because everything the page shows comes
//! over that one link.

use dioxus::prelude::*;

use crate::{profile, state::Ui, ui::pages::Page};

#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub enum View {
    #[default]
    Home,
    /// A computer's page.
    Computer(String),
    /// A settings page: of a computer (`Some(key)`, back goes to its page) or of this phone
    /// (`None`, back goes home).
    Sub(Option<String>, Page),
}

impl View {
    /// The computer this view is about, if any.
    pub fn computer(&self) -> Option<&str> {
        match self {
            View::Computer(k) | View::Sub(Some(k), _) => Some(k),
            _ => None,
        }
    }

    /// Where Back goes.
    pub fn back(&self) -> View {
        match self {
            // The three Virtual input pages sit under its hub.
            View::Sub(Some(k), p) if p.parent() == Page::Input => {
                View::Sub(Some(k.clone()), Page::Input)
            }
            View::Sub(Some(k), _) => View::Computer(k.clone()),
            _ => View::Home,
        }
    }
}

/// The index of the profile with this key.
pub fn index_of(ui: Ui, key: &str) -> Option<usize> {
    ui.set.profiles.read().iter().position(|p| p.key() == key)
}

/// Open a computer's page, selecting it first.
pub fn open_computer(ui: Ui, key: &str) {
    if let Some(i) = index_of(ui, key) {
        if (ui.set.profile)() != i {
            profile::select(ui, i);
        }
    }
    let mut view = ui.live.view;
    view.set(View::Computer(key.to_string()));
    // The card grows into the page (`js/hero_flip.js`), once Rust has rendered it.
    crate::bridge::call(format!(
        "window.__wado.heroOpen({});",
        crate::bridge::js(&key)
    ));
}

/// Push a settings page: the computer's when a computer page is up, else this phone's.
pub fn open_page(ui: Ui, page: Page) {
    let mut view = ui.live.view;
    let on = view.peek().computer().map(str::to_string);
    view.set(View::Sub(
        if page.about_computer() { on } else { None },
        page,
    ));
}

/// Open a settings page from wherever the person is: pushed inside the control centre while it
/// is up (a session), else pushed over home or the computer's page.
pub fn go(ui: Ui, page: Page) {
    let mut live = ui.live;
    if (live.cc_open)() {
        live.cc_page.set(page);
    } else {
        open_page(ui, page);
    }
}

pub fn back(ui: Ui) {
    let mut view = ui.live.view;
    let to = view.peek().back();
    view.set(to);
}

/// Where the person was, for a page that comes back after its process died (the Android app in
/// the background, a reload): home or a computer's page — a settings page comes back as its
/// computer's page. Never the session: that is only ever entered by a pick.
const VIEW_KEY: &str = "wado.view";
const VIEW_TTL_MS: u64 = 30 * 60 * 1000;

/// Keep [`VIEW_KEY`] up to date.
pub fn use_remember(ui: Ui) {
    use_effect(move || {
        if !(ui.live.loaded)() {
            return;
        }
        let view = (ui.live.view)();
        let at = match view.computer() {
            Some(k) => format!("c:{k}"),
            None => "home".to_string(),
        };
        crate::bridge::call(format!(
            "try {{ localStorage.setItem({}, JSON.stringify({{ at: {}, t: Date.now() }})); }} catch (_) {{}}",
            crate::bridge::js(&VIEW_KEY),
            crate::bridge::js(&at),
        ));
    });
}

/// Go back to where [`use_remember`] last saw the person, if that was recent — unless the
/// address asks for somewhere (`?go=`, handled by `deeplink`).
pub async fn restore(ui: Ui) {
    let code = format!(
        "try {{ if (/[?&]go=/.test(location.search)) return null;          const v = JSON.parse(localStorage.getItem({}) || 'null');          return v && Date.now() - v.t < {VIEW_TTL_MS} ? v.at : null; }} catch (_) {{ return null; }}",
        crate::bridge::js(&VIEW_KEY)
    );
    let Ok(v) = document::eval(&code).await else {
        return;
    };
    if let Some(key) = v.as_str().and_then(|s| s.strip_prefix("c:")) {
        if index_of(ui, key).is_some() {
            open_computer(ui, key);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn back_walks_up_one_level() {
        let k = "r|1".to_string();
        assert_eq!(
            View::Sub(Some(k.clone()), Page::SoundDevices).back(),
            View::Computer(k.clone())
        );
        assert_eq!(View::Computer(k.clone()).back(), View::Home);
        assert_eq!(
            View::Sub(Some(k.clone()), Page::InputGamepad).back(),
            View::Sub(Some(k.clone()), Page::Input)
        );
        assert_eq!(
            View::Sub(Some(k.clone()), Page::Input).back(),
            View::Computer(k.clone())
        );
        assert_eq!(View::Sub(None, Page::Appearance).back(), View::Home);
        assert_eq!(
            View::Sub(Some(k.clone()), Page::DisplaySession).computer(),
            Some("r|1")
        );
        assert_eq!(View::Home.computer(), None);
    }
}
