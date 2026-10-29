//! The check words of a typed pairing, confirmed on the computer (`2026-10-30`).
//!
//! A device that proves a typed code is not trusted yet: the daemon shows four check words
//! (`wado_protocol::envelope`), the device shows the four it made, and a person compares them
//! **on the computer** — the screen that cannot be impersonated. Yes pins the device; no, or
//! nobody answering within [`CONFIRM_WAIT`], refuses it. The code was spent when it was proven,
//! so either way it is gone.
//!
//! The daemon holding the device, the `wado pair` that showed the code and the `wado tui` panel
//! are separate processes of one machine, so the question is a file, keyed by the typed code:
//!
//! ```text
//!   <config>/wado/pair_confirm/<typed code>   <check>\t<words>\t<device>\t<since>
//! ```
//!
//! The daemon writes `ask`, a screen answers `yes` or `no`, the daemon settles it — `paired`,
//! `declined`, `no-answer` or `left` — so every screen can say how it ended.
//!
//! ponytail: polled, like the rest of the gate's files — a person answers in seconds, a third of
//! one is not felt.

use std::fs;
use std::path::PathBuf;
use std::time::Duration;

use tracing::warn;

use super::{Gate, now_s, typed_code};

/// How long a person has to compare the words before the answer counts as no.
pub const CONFIRM_WAIT: Duration = Duration::from_secs(120);
const POLL: Duration = Duration::from_millis(300);

/// Where a typed pairing's check stands.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Check {
    /// Waiting for someone at the computer.
    Asking,
    /// Answered at the computer; the daemon has not acted on it yet.
    Yes,
    No,
    /// Settled by the daemon.
    Paired,
    Declined,
    NoAnswer,
    /// The device went away before anyone answered.
    Left,
}

impl Check {
    fn as_str(self) -> &'static str {
        match self {
            Self::Asking => "ask",
            Self::Yes => "yes",
            Self::No => "no",
            Self::Paired => "paired",
            Self::Declined => "declined",
            Self::NoAnswer => "no-answer",
            Self::Left => "left",
        }
    }
    fn parse(s: &str) -> Option<Self> {
        [
            Self::Asking,
            Self::Yes,
            Self::No,
            Self::Paired,
            Self::Declined,
            Self::NoAnswer,
            Self::Left,
        ]
        .into_iter()
        .find(|c| c.as_str() == s)
    }
}

/// One typed pairing's question, as a screen shows it.
#[derive(Debug, Clone, PartialEq)]
pub struct Ask {
    /// The typed code it is for.
    pub typed: String,
    /// The four check words, space-separated.
    pub words: String,
    pub device: String,
    /// Unix seconds it was asked.
    pub since: u64,
    pub check: Check,
}

impl Ask {
    /// Seconds left to answer.
    pub fn left(&self) -> u64 {
        (self.since + CONFIRM_WAIT.as_secs()).saturating_sub(now_s())
    }
}

impl Gate {
    fn confirm_dir(&self) -> PathBuf {
        self.dir.join("pair_confirm")
    }

    /// Only a typed code becomes a file name.
    fn confirm_path(&self, typed: &str) -> Option<PathBuf> {
        (typed_code::normalize(typed).as_deref() == Some(typed))
            .then(|| self.confirm_dir().join(typed))
    }

    fn write_ask(&self, a: &Ask) -> bool {
        let Some(path) = self.confirm_path(&a.typed) else {
            return false;
        };
        let device = a.device.replace(['\t', '\n'], " ");
        let body = format!("{}\t{}\t{device}\t{}\n", a.check.as_str(), a.words, a.since);
        // Written whole and renamed in, so a screen never reads half an answer.
        let tmp = path.with_extension("tmp");
        let res = fs::create_dir_all(self.confirm_dir())
            .and_then(|_| fs::write(&tmp, body))
            .and_then(|_| fs::rename(&tmp, &path));
        if let Err(e) = &res {
            warn!("gate: could not write a pairing check: {e}");
        }
        res.is_ok()
    }

    /// Put the check words of a typed pairing to whoever is at the computer.
    pub fn ask(&self, typed: &str, words: &str, device: &str) -> bool {
        self.write_ask(&Ask {
            typed: typed.into(),
            words: words.into(),
            device: device.into(),
            since: now_s(),
            check: Check::Asking,
        })
    }

    /// The question for typed code `typed`, however it stands.
    pub fn ask_of(&self, typed: &str) -> Option<Ask> {
        let text = fs::read_to_string(self.confirm_path(typed)?).ok()?;
        let mut f = text.trim_end_matches('\n').split('\t');
        Some(Ask {
            check: Check::parse(f.next()?)?,
            words: f.next()?.into(),
            device: f.next()?.into(),
            since: f.next()?.parse().ok()?,
            typed: typed.into(),
        })
    }

    /// Every question still waiting for an answer, oldest first.
    pub fn asking(&self) -> Vec<Ask> {
        let Ok(dir) = fs::read_dir(self.confirm_dir()) else {
            return Vec::new();
        };
        let mut out: Vec<Ask> = dir
            .flatten()
            .filter_map(|e| self.ask_of(e.file_name().to_str()?))
            .filter(|a| a.check == Check::Asking && a.left() > 0)
            .collect();
        out.sort_by_key(|a| a.since);
        out
    }

    /// Answer at the computer. False unless the question is still waiting.
    pub fn answer(&self, typed: &str, yes: bool) -> bool {
        match self.ask_of(typed) {
            Some(a) if a.check == Check::Asking => self.write_ask(&Ask {
                check: if yes { Check::Yes } else { Check::No },
                ..a
            }),
            _ => false,
        }
    }

    /// The daemon's last word on a question, for the screens.
    pub fn settle(&self, typed: &str, check: Check) {
        if let Some(a) = self.ask_of(typed) {
            self.write_ask(&Ask { check, ..a });
        }
    }

    /// Wait for the answer to `typed`'s question: [`Check::Yes`], [`Check::No`],
    /// [`Check::NoAnswer`] after `wait`, or [`Check::Left`] once `still_wanted` turns false.
    pub async fn await_answer(
        &self,
        typed: &str,
        wait: Duration,
        still_wanted: impl Fn() -> bool,
    ) -> Check {
        let deadline = tokio::time::Instant::now() + wait;
        loop {
            match self.ask_of(typed).map(|a| a.check) {
                Some(c @ (Check::Yes | Check::No)) => return c,
                // Withdrawn under us: nobody can answer it any more.
                None => return Check::NoAnswer,
                _ => {}
            }
            if !still_wanted() {
                return Check::Left;
            }
            if tokio::time::Instant::now() >= deadline {
                return Check::NoAnswer;
            }
            tokio::time::sleep(POLL).await;
        }
    }

    /// Questions from long ago, swept when a new code is made.
    pub(super) fn sweep_asks(&self) {
        let Ok(dir) = fs::read_dir(self.confirm_dir()) else {
            return;
        };
        let old = super::PAIR_TTL.as_secs() + CONFIRM_WAIT.as_secs();
        for e in dir.flatten() {
            let stale = e
                .file_name()
                .to_str()
                .and_then(|t| self.ask_of(t))
                .is_none_or(|a| now_s().saturating_sub(a.since) > old);
            if stale {
                let _ = fs::remove_file(e.path());
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> Gate {
        Gate::at(std::env::temp_dir().join(format!("wado-confirm-{}", rand::random::<u64>())))
    }

    #[test]
    fn a_question_is_asked_answered_once_and_settled() {
        let g = temp();
        assert!(g.ask("K7QF2MXP", "desert badger torch shell", "Linux · Firefox"));
        let a = g.asking();
        assert_eq!(a.len(), 1);
        assert_eq!(a[0].words, "desert badger torch shell");
        assert_eq!(a[0].device, "Linux · Firefox");
        assert!(a[0].left() > 110);
        assert!(g.answer("K7QF2MXP", true));
        assert!(!g.answer("K7QF2MXP", false), "answered already");
        assert!(g.asking().is_empty(), "no longer waiting");
        g.settle("K7QF2MXP", Check::Paired);
        assert_eq!(g.ask_of("K7QF2MXP").unwrap().check, Check::Paired);
        assert!(!g.answer("K7QF2MXP", true), "settled");
    }

    #[test]
    fn only_a_typed_code_names_a_file() {
        let g = temp();
        assert!(!g.ask("../trusted_clients", "a b c d", "x"));
        assert!(!g.answer("../x", true));
        assert!(g.ask_of("k7qf2mxp").is_none(), "normalised codes only");
    }

    #[tokio::test]
    async fn the_daemon_hears_yes_no_silence_or_a_departure() {
        let g = temp();
        let short = Duration::from_millis(700);
        g.ask("AAAA1111", "w w w w", "d");
        let g2 = g.clone();
        let t = tokio::spawn(async move { g2.await_answer("AAAA1111", short * 4, || true).await });
        tokio::time::sleep(Duration::from_millis(100)).await;
        g.answer("AAAA1111", true);
        assert_eq!(t.await.unwrap(), Check::Yes);

        g.ask("BBBB2222", "w w w w", "d");
        g.answer("BBBB2222", false);
        assert_eq!(g.await_answer("BBBB2222", short, || true).await, Check::No);

        g.ask("CCCC3333", "w w w w", "d");
        assert_eq!(
            g.await_answer("CCCC3333", short, || true).await,
            Check::NoAnswer,
            "silence is a no"
        );

        g.ask("DDDD4444", "w w w w", "d");
        assert_eq!(
            g.await_answer("DDDD4444", short, || false).await,
            Check::Left
        );
    }
}
