//! The macOS providers: this Mac's capabilities behind `wado-platform`'s traits.
//!
//! Each capability is offered only when opening it works here (invariant 6); what is missing
//! says why, naming the System Settings pane when a permission is the reason.
#![cfg(target_os = "macos")]

pub mod cmd;
pub mod launch;
pub mod power;
pub mod sysinfo;
pub mod tcc;

use std::sync::{Arc, OnceLock};

use wado_platform::desktop::{Launcher, Screenshot};
use wado_platform::power::{PowerInhibit, SystemPower};
use wado_platform::sysinfo::SysInfoSource;
use wado_platform::{Offer, Provider, ProviderId};

pub const MACOS: ProviderId = ProviderId("macos");

/// An offer that is always here.
fn here<T: ?Sized>(imp: Arc<T>) -> Offer<T> {
    Some(Ok(imp))
}

/// An offer that is here when `probe` passes.
fn when<T: ?Sized>(probe: Result<(), String>, imp: impl FnOnce() -> Arc<T>) -> Offer<T> {
    Some(probe.map(|()| imp()))
}

/// Every macOS provider in one. A stateful capability is one instance for the daemon's life.
#[derive(Default)]
pub struct MacProvider {
    awake: OnceLock<Arc<power::MacAwake>>,
}

impl Provider for MacProvider {
    fn id(&self) -> ProviderId {
        MACOS
    }

    fn sys_info(&self) -> Offer<dyn SysInfoSource> {
        here(Arc::new(sysinfo::MacSysInfo))
    }

    fn power_inhibit(&self) -> Offer<dyn PowerInhibit> {
        here(
            self.awake
                .get_or_init(|| Arc::new(power::MacAwake::new()))
                .clone(),
        )
    }

    fn system_power(&self) -> Offer<dyn SystemPower> {
        here(Arc::new(power::MacPower))
    }

    fn launcher(&self) -> Offer<dyn Launcher> {
        here(Arc::new(launch::MacLauncher))
    }

    fn screenshot(&self) -> Offer<dyn Screenshot> {
        let probe = tcc::screen_recording()
            .then_some(())
            .ok_or_else(|| tcc::NEEDS_SCREEN_RECORDING.to_string());
        when(probe, || {
            Arc::new(launch::MacScreenshot) as Arc<dyn Screenshot>
        })
    }
}
