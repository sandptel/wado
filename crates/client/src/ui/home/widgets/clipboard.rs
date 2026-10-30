//! The Clipboard widget: its computer's Clipboard surface in short (`crate::ui::clipboard`, the
//! same component as the Computer page's) — the newest three, pinned first, each one tap onto
//! this phone or back onto the computer; and this phone's clipboard sent the other way. Its
//! title opens the computer's page, at its Clipboard section.

use dioxus::prelude::*;

use crate::{route, state::Ui, ui::clipboard::Clipboard as Surface};

use super::shell::Ctx;

/// The computer `pk`'s page, scrolled to its clipboard.
pub fn open_section(ui: Ui, pk: &str) {
    route::open_computer(ui, pk);
    crate::bridge::call(
        "setTimeout(() => { const s = document.getElementById('clipsec'); \
         if (s) s.scrollIntoView({ block: 'start', behavior: matchMedia('(prefers-reduced-motion: reduce)').matches ? 'auto' : 'smooth' }); }, 560);"
            .to_string(),
    );
}

#[component]
pub fn Clipboard(cx: Ctx) -> Element {
    rsx! {
        Surface {
            ui: cx.ui,
            pk: cx.w.host.clone(),
            name: cx.name.clone(),
            selected: cx.r.selected,
            compact: true,
        }
    }
}
