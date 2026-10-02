//! A session's own accessibility bus: the AT-SPI tree wado reads to know what is under a finger.
//!
//! One job: start an AT-SPI bus and its registry for this session, and hand applications its
//! address. The phone shell's precision features (magnetic taps, the lens, native menus) ask
//! applications through it which element sits at a point and how big it is.
//!
//! **Why not the usual way.** A desktop gets this bus from `at-spi-bus-launcher`, activated
//! through `org.a11y.Bus` on the session bus. From wado's private bus that activation reaches
//! the *host's* accessibility bus (`$XDG_RUNTIME_DIR/at-spi/bus_0`) — the session's apps then
//! register with the host desktop's registry, which is exactly the leak the private bus exists
//! to prevent — and the registry itself cannot be activated at all (its service goes through a
//! systemd user unit). Measured in the S5 spike, 2026-09-29. So wado runs both itself and
//! points applications at them with `AT_SPI_BUS_ADDRESS`, which GTK, Qt and the registry
//! honour before any bus lookup.
//!
//! Needs `WADO_ATSPI` (at-spi2-core's prefix — its binaries live in `libexec`, never on
//! `PATH`); the flake sets it. Without it the session runs with no accessibility tree, and the
//! precision features fall back to plain taps.

use std::process::{Child, Command, Stdio};

use tracing::{info, warn};

use super::bus::{self, SessionBus};

/// A running accessibility bus and its registry. Killed with the session.
pub struct A11yBus {
    bus: SessionBus,
    registry: Child,
}

impl A11yBus {
    /// The value for `AT_SPI_BUS_ADDRESS`.
    pub fn address(&self) -> &str {
        &self.bus.address
    }
}

/// Start the bus and its registry, or `None` (logged) when at-spi2-core is not available.
pub fn start() -> Option<A11yBus> {
    let Some(prefix) = wado_config::live::current().session.atspi.clone() else {
        warn!("session.atspi (WADO_ATSPI) is not set — no accessibility tree; taps stay plain");
        return None;
    };
    let conf = prefix.join("share/defaults/at-spi2/accessibility.conf");
    let bus = bus::daemon(
        &["--config-file", &conf.to_string_lossy()],
        "accessibility bus for this wado session",
    )?;
    let registry = match Command::new(prefix.join("libexec/at-spi2-registryd"))
        .env("AT_SPI_BUS_ADDRESS", &bus.address)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .spawn()
    {
        Ok(child) => child,
        Err(e) => {
            warn!("at-spi2-registryd did not start ({e}) — no accessibility tree");
            bus::terminate(bus);
            return None;
        }
    };
    info!(
        pid = registry.id(),
        "accessibility registry for this wado session"
    );
    Some(A11yBus { bus, registry })
}

/// Stop the registry and the bus.
pub fn terminate(mut a11y: A11yBus) {
    let _ = a11y.registry.kill();
    let _ = a11y.registry.wait();
    bus::terminate(a11y.bus);
}
