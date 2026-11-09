//! The platform layer (`wado-platform`, `docs/superpowers/specs/2026-10-30-platform-layer-design.md`)
//! as this daemon uses it: the providers for this OS, and the one probed [`Platform`] every part
//! of the daemon asks for a host capability.
//!
//! Policy stays out of here — grants, Wi-Fi probation, pacing, the viewer sink's lifecycle are
//! the daemon's own, written once over the traits. This is only "which provider, and is it here".
//!
//! - [`linux`] — the Linux providers.

#[cfg(target_os = "linux")]
pub mod linux;
#[cfg(target_os = "macos")]
pub use wado_host_macos as macos;

use std::sync::OnceLock;

use wado_platform::{Platform, PlatformBuilder};

static PLATFORM: OnceLock<Platform> = OnceLock::new();

/// This OS's providers, in the order they are asked.
#[cfg(target_os = "linux")]
pub fn os_default() -> PlatformBuilder {
    PlatformBuilder::empty().with(linux::LinuxProvider::default())
}

/// This OS's providers, in the order they are asked.
#[cfg(target_os = "macos")]
pub fn os_default() -> PlatformBuilder {
    PlatformBuilder::empty().with(macos::MacProvider::default())
}

/// The daemon's platform, probed on first use. The daemon probes it at start, after capturing
/// the desktop's display and Hyprland instance and before the compositor replaces them, so
/// providers that need the desktop find it.
pub fn get() -> &'static Platform {
    PLATFORM.get_or_init(|| os_default().probe())
}
