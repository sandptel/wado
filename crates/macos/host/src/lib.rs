//! The macOS providers: this Mac's capabilities behind `wado-platform`'s traits.
//!
//! Each capability is offered only when opening it works here (invariant 6); what is missing
//! says why, naming the System Settings pane when a permission is the reason.
#![cfg(target_os = "macos")]

use wado_platform::{Provider, ProviderId};

pub const MACOS: ProviderId = ProviderId("macos");

/// Every macOS provider in one.
#[derive(Default)]
pub struct MacProvider {}

impl Provider for MacProvider {
    fn id(&self) -> ProviderId {
        MACOS
    }
}
