//! Continue: what was done on each computer, newest first, and whether it was a game lately —
//! which decides between "Jump back in" (a carousel of games) and one Continue card.

use serde::{Deserialize, Serialize};

/// One thing done: `kind` is `game`, `desktop`, `shell` or `file`.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct Recent {
    #[serde(rename = "c")]
    pub computer: String,
    #[serde(rename = "k")]
    pub kind: String,
    #[serde(rename = "n")]
    pub name: String,
    #[serde(rename = "e")]
    pub exec: String,
    /// Unix seconds.
    #[serde(rename = "t")]
    pub at: u64,
}

/// Remember `r` at the front, the same thing once, at most 16.
pub fn note(list: &[Recent], r: Recent) -> Vec<Recent> {
    let mut out: Vec<Recent> = list
        .iter()
        .filter(|x| !(x.computer == r.computer && x.exec == r.exec && x.kind == r.kind))
        .cloned()
        .collect();
    out.insert(0, r);
    out.truncate(16);
    out
}

/// One of the last two things done on `computer`, or anything in three days, was a game.
pub fn gamed_lately(list: &[Recent], computer: &str, now: u64) -> bool {
    let mine: Vec<&Recent> = list.iter().filter(|r| r.computer == computer).collect();
    mine.iter().take(2).any(|r| r.kind == "game")
        || mine
            .iter()
            .any(|r| r.kind == "game" && now.saturating_sub(r.at) < 3 * 86_400)
}

/// "just now", "12 min ago", "3 h ago", "2 d ago".
pub fn ago(at: u64, now: u64) -> String {
    match now.saturating_sub(at) {
        s if s < 90 => "just now".into(),
        s if s < 3600 => format!("{} min ago", s / 60),
        s if s < 86_400 => format!("{} h ago", s / 3600),
        s => format!("{} d ago", s / 86_400),
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn r(c: &str, k: &str, e: &str, at: u64) -> Recent {
        Recent {
            computer: c.into(),
            kind: k.into(),
            name: e.into(),
            exec: e.into(),
            at,
        }
    }
    #[test]
    fn lately_and_dedup() {
        let l = note(
            &[r("pc", "desktop", "", 10), r("pc", "game", "hades", 5)],
            r("pc", "desktop", "", 20),
        );
        assert_eq!(l.len(), 2, "the same thing once");
        assert!(gamed_lately(&l, "pc", 20), "a game in the last two");
        let old = vec![
            r("pc", "shell", "a", 9_000_000),
            r("pc", "file", "b", 9_000_000),
            r("pc", "game", "x", 1),
        ];
        assert!(!gamed_lately(&old, "pc", 9_000_000));
        assert_eq!(ago(0, 7200), "2 h ago");
    }
}
