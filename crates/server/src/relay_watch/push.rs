//! Telling the devices when wado's relay goes away and comes back, while this computer stays up.
//!
//! Only the computer can say this. The relay cannot (it is what is gone), and a phone can only
//! see that it reaches nothing. Web Push goes from here straight to each device's push service,
//! not through the relay, so it still works — and if this computer is offline too, the push
//! simply does not go and nothing false was said. Once per pool per outage (`crate::push::once`),
//! to the devices with "computer online" notifications on.

use std::time::Duration;

use wado_protocol::push::{NOTE_RELAY, PushNote};

use super::record::{Record, now_ms, span};

/// Shorter than this is a relay restart or a network change — not news.
pub const LOST_FOR: Duration = Duration::from_secs(90);

/// Has the link been down long enough to say so, and not said yet?
pub fn lost_long_enough(r: &Record) -> bool {
    !r.up && !r.told && r.for_ms(now_ms()) >= LOST_FOR.as_millis() as u64
}

/// One outage is one bucket, shared by the pool's daemons (they lose the relay together).
fn bucket(since_ms: u64) -> u64 {
    since_ms / 600_000
}

pub fn relay_lost(r: &Record) {
    if !crate::push::once(&format!("relay-lost-{}", bucket(r.since_ms))) {
        return;
    }
    let (computer, name) = crate::push::computer();
    let host = r.url.split("://").nth(1).unwrap_or(&r.url);
    let host = host.split('/').next().unwrap_or(host).to_string();
    crate::push::deliver(
        PushNote {
            kind: NOTE_RELAY.into(),
            title: "wado's relay is unreachable — not your computer".into(),
            body: format!(
                "{name} is fine, but phones cannot reach it until {host} answers again. \
                 wado keeps trying."
            ),
            computer,
            computer_name: name,
            tag: "relay".into(),
            at: crate::passkey::now_s(),
            ..Default::default()
        },
        |e| e.kinds.online,
    );
}

pub fn relay_back(away_ms: u64) {
    if !crate::push::once(&format!(
        "relay-back-{}",
        bucket(now_ms().saturating_sub(away_ms))
    )) {
        return;
    }
    let (computer, name) = crate::push::computer();
    crate::push::deliver(
        PushNote {
            kind: NOTE_RELAY.into(),
            title: "wado's relay is back".into(),
            body: format!(
                "{name} can be reached again — it was out of reach for {}.",
                span(away_ms)
            ),
            computer,
            computer_name: name,
            // Replaces the "unreachable" notification on the device.
            tag: "relay".into(),
            at: crate::passkey::now_s(),
            ..Default::default()
        },
        |e| e.kinds.online,
    );
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn says_it_once_after_a_real_outage_only() {
        let now = now_ms();
        let mut r = Record {
            up: false,
            since_ms: now - 10_000,
            ..Default::default()
        };
        assert!(!lost_long_enough(&r), "a blip");
        r.since_ms = now - LOST_FOR.as_millis() as u64 - 1;
        assert!(lost_long_enough(&r));
        r.told = true;
        assert!(!lost_long_enough(&r), "once");
        r.told = false;
        r.up = true;
        assert!(!lost_long_enough(&r), "up");
    }
}
