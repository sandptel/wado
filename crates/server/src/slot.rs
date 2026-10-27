//! Which instance a new daemon is: the first number no live daemon answers to. Its UDP slice
//! follows it (`n - 1`), the way `scripts/rig.sh` numbers them. This is what lets a second
//! `wado daemon` on the same computer join the pool instead of colliding with the first.
//!
//! ponytail: two daemons started in the same instant can pick the same number; the relay
//! refuses the second. A lock file is the upgrade if that ever happens outside a test.

pub fn next() -> u32 {
    let live = crate::cli::status::daemons();
    (1..)
        .find(|n: &u32| !live.contains(&n.to_string()))
        .unwrap_or(1)
}

/// Before the config loads: take the next free instance unless one was chosen — by
/// `WADO_INSTANCE`, or by `server { instance }` in config.kdl.
pub fn claim() {
    if std::env::var_os("WADO_INSTANCE").is_some()
        || wado_config::load_or_init().is_ok_and(|l| {
            l.config.server.instance != wado_config::schema::server::Server::default().instance
        })
    {
        return;
    }
    let n = next();
    // SAFETY: single-threaded — nothing has started yet (logging, config watcher, compositor).
    unsafe {
        std::env::set_var("WADO_INSTANCE", n.to_string());
        std::env::set_var("WADO_UDP_SLICE", (n - 1).to_string());
    }
}
