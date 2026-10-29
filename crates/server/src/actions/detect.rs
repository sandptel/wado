//! What an action needs here, as platform capabilities (`wado_platform`), and asking the
//! platform whether this computer has them.
//!
//! The platform probes once; actions look again at most every [`FRESH`], and after a miss — an
//! action that failed or was asked for while missing — so a Hyprland restarted or a tool just
//! installed shows up on the next list.

use std::sync::Mutex;
use std::time::{Duration, Instant};

use wado_platform::desktop::App;
use wado_platform::power::PowerOp;
use wado_platform::{Capability, Platform};

/// How long what was probed is trusted for listing.
pub const FRESH: Duration = Duration::from_secs(30);

/// One thing an action needs here.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Need {
    /// The capability is provided.
    Cap(Capability),
    /// The platform's system power can do this.
    Power(PowerOp),
    /// The platform's launcher can open this.
    Launch(App),
    /// The platform's screenshots can be of an area the person chooses.
    Region,
}

impl Need {
    /// The capability this need is part of.
    pub fn cap(&self) -> Capability {
        match self {
            Need::Cap(c) => *c,
            Need::Power(_) => Capability::SystemPower,
            Need::Launch(_) => Capability::Launcher,
            Need::Region => Capability::Screenshot,
        }
    }
}

/// Answers "is this here, and if not, why?" — the platform, or a fake in tests.
pub trait Detect {
    fn check(&self, need: &Need) -> Result<(), String>;
}

impl Detect for Platform {
    fn check(&self, need: &Need) -> Result<(), String> {
        let absent = |a: wado_platform::Absent| a.why;
        match need {
            Need::Cap(c) => self.absent(*c).map_or(Ok(()), |a| Err(a.why)),
            Need::Power(op) => self.system_power().map_err(absent)?.can(*op),
            Need::Launch(app) => self.launcher().map_err(absent)?.can(*app),
            Need::Region => self.screenshot().map_err(absent)?.can_region(),
        }
    }
}

/// When the actions' capabilities were last probed.
static LOOKED: Mutex<Option<Instant>> = Mutex::new(None);

/// Probe the actions' capabilities again when they were last looked at more than [`FRESH`] ago.
pub async fn fresh(p: &'static Platform) {
    let stale = LOOKED
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .is_none_or(|at| at.elapsed() >= FRESH);
    if stale {
        look_again(p, super::used()).await;
        *LOOKED.lock().unwrap_or_else(|e| e.into_inner()) = Some(Instant::now());
    }
}

/// Look again on the next [`fresh`].
pub fn forget() {
    *LOOKED.lock().unwrap_or_else(|e| e.into_inner()) = None;
}

/// Probe `caps` again now. Probing may run a tool and wait for it, so it runs where blocking is
/// fine.
pub async fn look_again(p: &'static Platform, caps: Vec<Capability>) {
    let _ = tokio::task::spawn_blocking(move || {
        for cap in caps {
            p.reprobe(cap);
        }
    })
    .await;
}
