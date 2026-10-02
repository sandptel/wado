//! Notifications from the session's apps, shown on the viewer.
//!
//! An isolated session has its own D-Bus bus with no notification daemon on it, so an app's
//! "download finished" went nowhere. This serves `org.freedesktop.Notifications` on that bus
//! and forwards each one to whoever is watching. With isolation off, apps reach the host's own
//! notification daemon, and this stays out of its way.
//!
//! ponytail: summary, body and app name only — no actions, icons or images. Actions are the
//! upgrade (a button on the phone calling back `ActionInvoked`) once something needs them.

use std::sync::{
    OnceLock,
    atomic::{AtomicU32, Ordering},
};

use tokio::sync::{broadcast, watch};
use zbus::interface;

#[derive(Clone, Debug)]
pub enum Event {
    Shown {
        id: u32,
        app: String,
        summary: String,
        body: String,
    },
    Closed {
        id: u32,
    },
}

fn bus() -> &'static broadcast::Sender<Event> {
    static TX: OnceLock<broadcast::Sender<Event>> = OnceLock::new();
    TX.get_or_init(|| broadcast::channel(64).0)
}

pub fn events() -> broadcast::Receiver<Event> {
    bus().subscribe()
}

static NEXT: AtomicU32 = AtomicU32::new(1);

/// The session's private bus while there is one — also where `crate::host::media` looks for the
/// session's own media players.
static SESSION_BUS: std::sync::Mutex<Option<String>> = std::sync::Mutex::new(None);

pub fn session_bus() -> Option<String> {
    SESSION_BUS
        .lock()
        .unwrap_or_else(|e| e.into_inner())
        .clone()
}

struct Notifications;

#[interface(name = "org.freedesktop.Notifications")]
impl Notifications {
    #[allow(clippy::too_many_arguments)]
    fn notify(
        &self,
        app_name: String,
        replaces_id: u32,
        _app_icon: String,
        summary: String,
        body: String,
        _actions: Vec<String>,
        _hints: std::collections::HashMap<String, zbus::zvariant::OwnedValue>,
        _expire_timeout: i32,
    ) -> u32 {
        let id = if replaces_id != 0 {
            replaces_id
        } else {
            NEXT.fetch_add(1, Ordering::Relaxed)
        };
        let _ = bus().send(Event::Shown {
            id,
            app: app_name,
            summary,
            body,
        });
        id
    }

    fn close_notification(&self, id: u32) {
        let _ = bus().send(Event::Closed { id });
    }

    fn get_capabilities(&self) -> Vec<String> {
        vec!["body".into()]
    }

    fn get_server_information(&self) -> (String, String, String, String) {
        (
            "wado".into(),
            "wado".into(),
            env!("CARGO_PKG_VERSION").into(),
            "1.2".into(),
        )
    }
}

/// Follow the session bus: serve on it while there is one. Runs for the life of the daemon.
pub async fn run(mut addr: watch::Receiver<Option<String>>) {
    loop {
        let now = addr.borrow_and_update().clone();
        *SESSION_BUS.lock().unwrap_or_else(|e| e.into_inner()) = now.clone();
        // Held for this turn of the loop: the next change drops it, releasing the name on a
        // bus that is going away anyway.
        let _conn = match now {
            Some(a) => match serve(&a).await {
                Ok(c) => {
                    tracing::info!("serving notifications on the session bus");
                    Some(c)
                }
                Err(e) => {
                    tracing::warn!("could not serve notifications on the session bus: {e}");
                    None
                }
            },
            None => None,
        };
        if addr.changed().await.is_err() {
            break;
        }
    }
}

async fn serve(address: &str) -> zbus::Result<zbus::Connection> {
    zbus::connection::Builder::address(address)?
        .serve_at("/org/freedesktop/Notifications", Notifications)?
        .name("org.freedesktop.Notifications")?
        .build()
        .await
}
