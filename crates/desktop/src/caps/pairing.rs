// SPDX-License-Identifier: AGPL-3.0-only
//! Pairing codes and their check words, through the daemon's own code: `wado::cli::qr::pair_link`
//! mints a code and builds the connect link exactly as `wado pair` does, and `wado::gate` says
//! what became of it and holds the four-word question (`gate/confirm.rs`).

pub use wado::gate::{CodeState, Grants, Minted, PAIR_TTL, confirm::Ask};
use wado::gate::{Gate, typed_code};

/// A fresh code and the link that carries it.
pub struct Pairing {
    pub link: String,
    pub code: Minted,
}

/// A new pairing code carrying `grants`. Fails with the CLI's own words when there is no public
/// relay URL to put in the link.
pub fn mint(grants: &Grants) -> Result<Pairing, String> {
    wado::cli::qr::pair_link(None, None, grants).map(|p| Pairing {
        link: p.link,
        code: p.code,
    })
}

pub fn state(code: &Minted) -> CodeState {
    Gate::default().state(code)
}

/// Withdraw a code unused.
pub fn withdraw(code: &Minted) {
    Gate::default().revoke_pair(&code.code);
}

/// Typed pairings waiting for someone here to compare their check words, oldest first.
pub fn asking() -> Vec<Ask> {
    Gate::default().asking()
}

/// Answer the check words. False unless the question was still waiting.
pub fn answer(typed: &str, yes: bool) -> bool {
    Gate::default().answer(typed, yes)
}

/// `K7QF2MXP` → `K7QF-2MXP`, as a person types it.
pub fn display(typed: &str) -> String {
    typed_code::display(typed)
}

/// `security { new-device }`: what a pairing checklist starts from.
pub fn new_device_grants() -> Grants {
    Grants::new_device()
}

/// The QR's modules, row by row: `(width, dark?)`.
pub fn qr(link: &str) -> Option<(usize, Vec<bool>)> {
    let code = qrcode::QrCode::new(link.as_bytes()).ok()?;
    let w = code.width();
    let dark = code
        .to_colors()
        .into_iter()
        .map(|c| c == qrcode::Color::Dark)
        .collect();
    Some((w, dark))
}
