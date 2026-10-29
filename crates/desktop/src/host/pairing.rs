// SPDX-License-Identifier: AGPL-3.0-only
//! Pairing: one code on screen at a time, as in `wado tui`. A code is made when the person asks
//! (opening the page the first time, New code, or changing the checklist); a used code is
//! replaced, ready for the next device; an expired or withdrawn one is not, so a console left
//! open holds no live way in. The check words of any typed pairing — from this page, `wado
//! pair` in a terminal, or `wado tui` — come up as a sheet over whatever page is open.

use slint::{ComponentHandle, Image, Rgba8Pixel, SharedPixelBuffer};

use super::Ctl;
use crate::{
    AppWindow, AskRow, Host, Theme,
    caps::{
        devices::Grants,
        pairing::{self, CodeState, PAIR_TTL},
        watch::{HostSnapshot, ShownCode},
    },
    models::{strings, sync},
};

pub struct State {
    shown: ShownCode,
    link: String,
    grants: Grants,
    /// The link the QR image was drawn for, and with which colours.
    qr_for: (String, [u8; 8]),
    /// A code has been asked for at least once this run.
    asked: bool,
}

impl State {
    pub fn shown(&self) -> ShownCode {
        self.shown.clone()
    }
    pub fn new(shown: ShownCode) -> Self {
        Self {
            shown,
            link: String::new(),
            grants: pairing::new_device_grants(),
            qr_for: (String::new(), [0; 8]),
            asked: false,
        }
    }
}

pub fn wire(ui: &AppWindow) {
    let h = ui.global::<Host>();
    let weak = ui.as_weak();
    h.on_mint(move || {
        if let Some(ui) = weak.upgrade() {
            mint(&ui);
        }
    });
    let weak = ui.as_weak();
    h.on_withdraw(move || {
        let Some(ui) = weak.upgrade() else { return };
        super::with(|c| {
            if let Some(m) = c.pairing.shown.lock().ok().and_then(|m| m.clone()) {
                pairing::withdraw(&m);
            }
            c.poke();
        });
        crate::toast::say(&ui, "Code withdrawn — it no longer works", "good");
    });
    let weak = ui.as_weak();
    h.on_pair_grant(move |token, on| {
        let Some(ui) = weak.upgrade() else { return };
        super::with(|c| {
            let _ = c.pairing.grants.set(&token, on);
        });
        mint(&ui);
    });
    let weak = ui.as_weak();
    h.on_pair_files(move |level| {
        let Some(ui) = weak.upgrade() else { return };
        super::with(|c| {
            let t = match level {
                1 => "files-ro",
                2 => "files-rw",
                _ => "files",
            };
            let _ = c.pairing.grants.set(t, level > 0);
        });
        mint(&ui);
    });
    let weak = ui.as_weak();
    h.on_answer(move |typed, yes| {
        let Some(ui) = weak.upgrade() else { return };
        if pairing::answer(&typed, yes) {
            crate::toast::say(
                &ui,
                if yes {
                    "Confirmed — the device is being paired"
                } else {
                    "Declined — that device is not paired"
                },
                if yes { "good" } else { "warn" },
            );
        } else {
            crate::toast::say(&ui, "That question had already ended", "warn");
        }
        super::with(|c| c.poke());
    });
    // The first visit to the page makes a code; one is never made unasked at start.
    let weak = ui.as_weak();
    h.on_pair_opened(move || {
        let Some(ui) = weak.upgrade() else { return };
        if super::with(|c| !c.pairing.asked).unwrap_or(false) {
            mint(&ui);
        }
    });
}

/// Make a fresh code with the checklist on screen, replacing (and withdrawing) the old one.
pub fn mint(ui: &AppWindow) {
    let res = super::with(|c| {
        c.pairing.asked = true;
        if let Some(old) = c.pairing.shown.lock().ok().and_then(|m| m.clone()) {
            if matches!(pairing::state(&old), CodeState::Live(_)) {
                pairing::withdraw(&old);
            }
        }
        let r = pairing::mint(&c.pairing.grants);
        if let Ok(p) = &r {
            c.pairing.link = p.link.clone();
            if let Ok(mut s) = c.pairing.shown.lock() {
                *s = Some(p.code.clone());
            }
        }
        c.poke();
        r.map(|p| (p.code, p.link))
    });
    let h = ui.global::<Host>();
    match res {
        Some(Ok((code, link))) => {
            h.set_pair_error("".into());
            show(ui, &code.typed, &link, &CodeState::Live(PAIR_TTL.as_secs()));
        }
        Some(Err(e)) => {
            h.set_pair_error(e.into());
            h.set_pair_ready(false);
        }
        None => {}
    }
    sync_grants(ui);
}

fn sync_grants(ui: &AppWindow) {
    let h = ui.global::<Host>();
    if let Some(g) = super::with(|c| c.pairing.grants.clone()) {
        h.set_pair_grants(super::devices::grants(&g));
        h.set_pair_grants_words(g.tokens().replace(',', " ").into());
    }
}

pub fn apply(ui: &AppWindow, ctl: &mut Ctl, s: &HostSnapshot) {
    let asks: Vec<AskRow> = s
        .asks
        .iter()
        .map(|a| {
            let left = a.left();
            AskRow {
                typed: a.typed.as_str().into(),
                words: strings(a.words.split_whitespace().map(str::to_string)),
                device: a.device.as_str().into(),
                left: left as i32,
                left_text: format!("{}:{:02}", left / 60, left % 60).into(),
            }
        })
        .collect();
    sync(&ctl.models.asks, asks);

    if !ctl.pairing.asked {
        sync_grants_from(ui, &ctl.pairing.grants);
        return;
    }
    let Some((code, state)) = &s.pair else { return };
    // A used code is replaced at once — ready for the next device.
    if *state == CodeState::Used {
        if let Ok(mut m) = ctl.pairing.shown.lock() {
            if m.as_ref() == Some(code) {
                *m = None;
            }
        }
        let ui = ui.as_weak();
        slint::Timer::single_shot(std::time::Duration::from_millis(0), move || {
            if let Some(ui) = ui.upgrade() {
                crate::toast::say(
                    &ui,
                    "Paired — here is a fresh code for the next device",
                    "good",
                );
                mint(&ui);
            }
        });
        return;
    }
    let link = ctl.pairing.link.clone();
    show_with(ui, ctl, &code.typed, &link, state);
}

fn sync_grants_from(ui: &AppWindow, g: &Grants) {
    let h = ui.global::<Host>();
    h.set_pair_grants(super::devices::grants(g));
    h.set_pair_grants_words(g.tokens().replace(',', " ").into());
}

fn show(ui: &AppWindow, typed: &str, link: &str, st: &CodeState) {
    super::with(|c| show_with(ui, c, typed, link, st));
}

fn show_with(ui: &AppWindow, c: &mut Ctl, typed: &str, link: &str, st: &CodeState) {
    let h = ui.global::<Host>();
    let live = matches!(st, CodeState::Live(_));
    h.set_pair_ready(true);
    h.set_pair_live(live);
    h.set_pair_code(pairing::display(typed).into());
    h.set_pair_link(link.into());
    h.set_pair_left(match st {
        CodeState::Live(s) => *s as i32,
        _ => 0,
    });
    h.set_pair_total(PAIR_TTL.as_secs() as i32);
    let (words, tone) = match st {
        CodeState::Live(_) => ("Live — works once, for one device".to_string(), "accent"),
        CodeState::Confirm { device, .. } => (
            format!("{device} typed it — compare the check words"),
            "caution",
        ),
        other => (sentence(&other.say()), "dim"),
    };
    h.set_pair_words(words.into());
    h.set_pair_tone(tone.into());

    // Draw the QR only when the link or the scheme changed.
    let t = ui.global::<Theme>();
    let (d, l) = (t.get_qr_dark(), t.get_qr_light());
    let key = [
        d.red(),
        d.green(),
        d.blue(),
        d.alpha(),
        l.red(),
        l.green(),
        l.blue(),
        l.alpha(),
    ];
    if c.pairing.qr_for != (link.to_string(), key) {
        if let Some(img) = qr_image(link, d, l) {
            h.set_pair_qr(img);
            c.pairing.qr_for = (link.to_string(), key);
        }
    }
}

/// Redraw the QR in new colours (after a scheme change).
pub fn recolour(ui: &AppWindow) {
    let (typed, link) = match super::with(|c| {
        c.pairing.qr_for.0.clear();
        (
            c.pairing.shown.lock().ok().and_then(|m| m.clone()),
            c.pairing.link.clone(),
        )
    }) {
        Some((Some(m), l)) => (m.typed, l),
        _ => return,
    };
    let st = super::with(|c| {
        c.last()
            .and_then(|s| s.pair.as_ref().map(|p| p.1.clone()))
            .unwrap_or(CodeState::Live(PAIR_TTL.as_secs()))
    })
    .unwrap_or(CodeState::Live(0));
    show(ui, &typed, &link, &st);
}

fn sentence(s: &str) -> String {
    let mut c = s.chars();
    match c.next() {
        Some(f) => f.to_uppercase().collect::<String>() + c.as_str(),
        None => String::new(),
    }
}

/// The QR as an image: one pixel per module plus a 2-module quiet zone, drawn `pixelated`
/// so it stays crisp at any size.
fn qr_image(link: &str, dark: slint::Color, light: slint::Color) -> Option<Image> {
    let (w, cells) = pairing::qr(link)?;
    let q = 2;
    let side = w + 2 * q;
    let mut buf = SharedPixelBuffer::<Rgba8Pixel>::new(side as u32, side as u32);
    let px = buf.make_mut_slice();
    let to = |c: slint::Color| Rgba8Pixel {
        r: c.red(),
        g: c.green(),
        b: c.blue(),
        a: 255,
    };
    for y in 0..side {
        for x in 0..side {
            let inside = x >= q && y >= q && x < q + w && y < q + w;
            let on = inside && cells[(y - q) * w + (x - q)];
            px[y * side + x] = to(if on { dark } else { light });
        }
    }
    Some(Image::from_rgba8(buf))
}
