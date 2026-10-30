//! This daemon's link to its relay, as it happens: dialled, registered, lost — kept here, written
//! out as a [`Record`] for the CLI and the TUI, and checked every few seconds for an outage long
//! enough to tell the devices about (`super::push`).

use std::sync::Mutex;
use std::time::Duration;

use super::record::{self, Record, now_ms};

static NOW: Mutex<Option<Record>> = Mutex::new(None);

/// How often the record is rewritten (its age is the heartbeat) and the outage checked.
const BEAT: Duration = Duration::from_secs(5);

fn with<T>(f: impl FnOnce(&mut Record) -> T) -> Option<T> {
    let mut g = NOW.lock().unwrap_or_else(|e| e.into_inner());
    let r = g.as_mut()?;
    let out = f(r);
    r.at_ms = now_ms();
    record::write(r);
    Some(out)
}

/// The daemon is about to dial `url` for the first time. Starts the heartbeat. Call once.
pub fn start(url: &str) {
    *NOW.lock().unwrap_or_else(|e| e.into_inner()) = Some(Record {
        instance: crate::instance::instance_key(),
        url: url.to_string(),
        since_ms: now_ms(),
        ..Default::default()
    });
    with(|_| ());
    tokio::spawn(async {
        let mut tick = tokio::time::interval(BEAT);
        loop {
            tick.tick().await;
            let told = with(|r| {
                if super::push::lost_long_enough(r) {
                    r.told = true;
                    true
                } else {
                    false
                }
            });
            if told == Some(true) {
                super::push::relay_lost(&current().unwrap_or_default());
            }
        }
    });
}

/// A copy of the link as it stands.
pub fn current() -> Option<Record> {
    NOW.lock().unwrap_or_else(|e| e.into_inner()).clone()
}

/// A dial or a registration failed (the link was never up this time).
pub fn failed(why: &str) {
    with(|r| r.error = why.chars().take(200).collect());
}

/// Held while registered; dropping it marks the link lost.
pub struct Up;

impl Drop for Up {
    fn drop(&mut self) {
        with(|r| {
            r.up = false;
            r.since_ms = now_ms();
            r.outages += 1;
            r.error = "the link to the relay closed".into();
        });
    }
}

/// Registered with the relay, which said it can do `caps`.
pub fn registered(caps: &[String]) -> Up {
    let back = with(|r| {
        let away = (!r.up).then(|| r.for_ms(now_ms()));
        let told = std::mem::take(&mut r.told);
        if r.outages > 0
            && let Some(a) = away
        {
            r.longest_ms = r.longest_ms.max(a);
        }
        r.up = true;
        r.since_ms = now_ms();
        r.error.clear();
        r.caps = caps.to_vec();
        (told, away.unwrap_or(0))
    });
    if let Some((true, away)) = back {
        super::push::relay_back(away);
    }
    Up
}

/// Did this outage last long enough that the devices were told the relay was gone? Read by
/// `crate::push::online`, which then leaves "back" to [`super::push::relay_back`].
pub fn told() -> bool {
    current().is_some_and(|r| r.told)
}
