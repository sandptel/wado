//! This daemon's identity within its pool, as the relay sees it.
//!
//! - **`instance_key`**: `WADO_INSTANCE`, default `1`. Stable across restarts of this daemon
//!   *and* of the relay, which is what returns a client to the same daemon — and the desktop
//!   still running on it — after the relay restarts. `scripts/rig.sh` numbers its daemons
//!   1..N; two daemons sharing a key on one Remote ID are refused by the relay.
//! - **`boot_id`**: fresh per process. Tells the relay a redial from a second process, and
//!   tells the client a daemon restart (its apps are gone) from a reconnect (they are not).

use std::sync::OnceLock;

use rand::Rng;

pub fn instance_key() -> String {
    std::env::var("WADO_INSTANCE")
        .ok()
        .filter(|k| !k.trim().is_empty())
        .unwrap_or_else(|| "1".into())
}

pub fn boot_id() -> &'static str {
    static ID: OnceLock<String> = OnceLock::new();
    ID.get_or_init(|| format!("{:016x}", rand::thread_rng().r#gen::<u64>()))
}
