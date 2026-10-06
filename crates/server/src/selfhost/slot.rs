//! Which instance a new daemon is: the first number no live daemon answers to. Its UDP slice
//! follows it (`n - 1`), the way `scripts/rig.sh` numbers them.
//!
//! ponytail: two daemons started in the same instant can pick the same number; the relay
//! refuses the second. A lock file is the upgrade if that ever happens outside a test.

pub fn next() -> u32 {
    let live = crate::cli::status::daemons();
    (1..)
        .find(|n: &u32| !live.contains(&n.to_string()))
        .unwrap_or(1)
}
