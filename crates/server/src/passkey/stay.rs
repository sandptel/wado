//! How long an unlock lasts — each device's own choice (Decision Log `2026-10-30`): 15 minutes
//! without use (the default), an hour, eight hours, or until it locks itself; and whether leaving
//! the app locks it. Changed only while unlocked (`relay`), so choosing needs the passkey once.
//!
//! On file beside the device's passkey (`<passkeys>/<device>.stay`, [`super::Store`]), not in
//! `config.kdl`: it is security state that must go with the passkey — `wado passkey remove` and
//! unpairing delete both — and a device's own config writes need no unlock, this does.
//!
//! What a choice means for the window ([`How`]):
//!
//! - **idle**: the chosen idle clock, or none for "until I lock it";
//! - **keep**: a non-default choice outlives the link that proved it — a reconnect of the same
//!   device finds it still open. Only on a line with a proven key: the envelope then proves it is
//!   the same device, which is what the unlock is bound to;
//! - **persist**: "until I lock it" also outlives a daemon restart (`open_since` on file), only
//!   on a line pinned by a pairing code. The timed choices do not: a restart is a fair moment to
//!   ask again, and their idle clock lives in memory.

use std::time::Duration;

use serde::{Deserialize, Serialize};
use wado_protocol::passkey::{STAY_DEFAULT, STAY_LOCK, stay_idle_s};

/// One device's choice, on file.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Prefs {
    /// Its device key, so a file named by a hash can be read back to whom it belongs.
    #[serde(default)]
    pub key: String,
    /// A name from `wado_protocol::passkey::STAYS`.
    pub stay: String,
    #[serde(default = "wado_protocol::passkey::default_lock_on_leave")]
    pub lock_on_leave: bool,
    /// Unix seconds it was unlocked, while "until I lock it" holds it open across restarts.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub open_since: Option<u64>,
}

impl Default for Prefs {
    fn default() -> Self {
        Self {
            key: String::new(),
            stay: STAY_DEFAULT.into(),
            lock_on_leave: true,
            open_since: None,
        }
    }
}

impl Prefs {
    /// The idle clock: `None` for "until I lock it". A name this daemon does not know (a file
    /// from a newer one) is read as the default.
    pub fn idle(&self) -> Option<Duration> {
        match stay_idle_s(&self.stay) {
            Some(0) => None,
            Some(s) => Some(Duration::from_secs(u64::from(s))),
            None => Some(super::unlock::IDLE),
        }
    }

    pub fn is_default(&self) -> bool {
        self.idle() == Some(super::unlock::IDLE)
    }

    pub fn until_locked(&self) -> bool {
        self.stay == STAY_LOCK
    }
}

/// What a device's window does, from its choice and its line on the trust list.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct How {
    pub idle: Option<Duration>,
    /// Outlives the link that proved it.
    pub keep: bool,
    /// Outlives a daemon restart.
    pub persist: bool,
}

impl Default for How {
    fn default() -> Self {
        Self {
            idle: Some(super::unlock::IDLE),
            keep: false,
            persist: false,
        }
    }
}

/// `proven`: the device's line holds a key its envelope proved. `pinned`: proven with a code.
pub fn how(p: &Prefs, proven: bool, pinned: bool) -> How {
    let chosen = !p.is_default();
    How {
        idle: p.idle(),
        keep: chosen && proven,
        persist: p.until_locked() && proven && pinned,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn prefs(stay: &str) -> Prefs {
        Prefs {
            stay: stay.into(),
            ..Prefs::default()
        }
    }

    #[test]
    fn each_choice_and_what_it_outlives() {
        let d = how(&Prefs::default(), true, true);
        assert_eq!(d, How::default(), "the default: 15 min, bound to its link");
        let h = how(&prefs("1h"), true, true);
        assert_eq!(h.idle, Some(Duration::from_secs(3600)));
        assert!(h.keep && !h.persist, "timed: a reconnect, not a restart");
        assert_eq!(
            how(&prefs("8h"), true, false).idle,
            Some(Duration::from_secs(8 * 3600))
        );
        let l = how(&prefs("lock"), true, true);
        assert!(l.idle.is_none() && l.keep && l.persist);
        let unpinned = how(&prefs("lock"), true, false);
        assert!(
            unpinned.keep && !unpinned.persist,
            "a restart needs a code-pinned line"
        );
        let unproven = how(&prefs("lock"), false, false);
        assert!(
            !unproven.keep && !unproven.persist,
            "no proven key: nothing outlives the link"
        );
        assert_eq!(
            how(&prefs("someday"), true, true),
            How::default(),
            "an unknown name is the default"
        );
    }

    #[test]
    fn reads_an_older_file() {
        let p: Prefs = serde_json::from_str(r#"{"stay":"1h"}"#).unwrap();
        assert!(p.lock_on_leave && p.open_since.is_none());
    }
}
