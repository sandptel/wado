//! Relay health, as this computer sees it (F4, Decision Log `2026-10-27`: "single VPS + uptime
//! monitoring"). The relay is one public box; when it goes, every phone loses every computer at
//! once, and each phone can only say it reaches nothing. The computer knows better — it is up,
//! and it is the relay that is gone — so it records its link, says so in `wado relay status`, `wado`
//! and `wado tui`, and pushes the devices when an outage is real.
//!
//! | file | job |
//! |---|---|
//! | [`record`] | the link as a file per daemon, and reading them back |
//! | [`state`] | the link as it happens, and the heartbeat |
//! | [`push`] | "wado's relay is unreachable — not your computer", and "back" |
//!
//! The outside view — is the relay up for anyone — is the uptime check's
//! (`deploy/relay/uptime.md`, `scripts/relay-health.sh`).

pub mod push;
pub mod record;
pub mod state;
