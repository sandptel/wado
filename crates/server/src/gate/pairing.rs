//! Pairing codes: how a new device gets in. Nothing else lets one in (`2026-10-27`).
//!
//! A code is made only when someone asks on this computer (`wado pair`, `wado tui` → Pair), lives
//! [`PAIR_TTL`] and works once. Each record has two forms of the same code: the long one the QR
//! link carries, and a short [`super::typed_code`] one to type. Redeeming either spends the record.
//!
//! The typed form is short, so guesses are counted per computer, not per device: after
//! [`MAX_STRIKES`] wrong ones every live code is revoked and a new one has to be made.
//!
//! ```text
//!   <config>/wado/pair_codes     <qr code>\t<expiry>\t<grants>[\t<typed code>]
//!   <config>/wado/pair_strikes   one line per wrong typed code since codes were last all gone
//!   <config>/wado/pair_lockout   present once the strikes revoked everything; cleared by a mint
//! ```

use std::fs;
use std::io::Write;
use std::path::PathBuf;
use std::time::Duration;

use tracing::warn;

use super::{Gate, Grants, now_s, typed_code};

/// How long a pairing code works.
pub const PAIR_TTL: Duration = Duration::from_secs(10 * 60);
/// Wrong typed codes before every live code is revoked.
pub const MAX_STRIKES: usize = 5;

/// A code just made, in both its forms.
#[derive(Debug, Clone, PartialEq)]
pub struct Minted {
    /// The long code, for the QR link.
    pub code: String,
    /// The short code, normalised; [`typed_code::display`] shows it.
    pub typed: String,
    /// Unix seconds it stops working.
    pub until: u64,
}

/// One line of `pair_codes`.
struct Record {
    code: String,
    until: u64,
    grants: String,
    typed: String,
}

impl Record {
    fn line(&self) -> String {
        format!(
            "{}\t{}\t{}\t{}",
            self.code, self.until, self.grants, self.typed
        )
    }
    fn live(&self) -> bool {
        self.until > now_s()
    }
    /// Either form of the code. An empty code matches nothing.
    fn is(&self, code: &str) -> bool {
        !code.is_empty() && (self.code == code || (!self.typed.is_empty() && self.typed == code))
    }
}

impl Gate {
    fn pairs_path(&self) -> PathBuf {
        self.dir.join("pair_codes")
    }
    fn strikes_path(&self) -> PathBuf {
        self.dir.join("pair_strikes")
    }
    fn lockout_path(&self) -> PathBuf {
        self.dir.join("pair_lockout")
    }

    /// A line from before checklists carries `new-device`; one from before typed codes has none.
    fn records(&self) -> Vec<Record> {
        fs::read_to_string(self.pairs_path())
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let mut f = l.split('\t');
                let code = f.next()?.to_string();
                let until = f.next()?.parse().ok()?;
                let grants = f
                    .next()
                    .map_or_else(|| Grants::new_device().tokens(), String::from);
                let typed = f.next().unwrap_or("").to_string();
                Some(Record {
                    code,
                    until,
                    grants,
                    typed,
                })
            })
            .collect()
    }

    fn write_records(&self, rs: &[Record]) {
        let body: String = rs.iter().map(|r| r.line() + "\n").collect();
        if let Err(e) =
            fs::create_dir_all(&self.dir).and_then(|_| fs::write(self.pairs_path(), body))
        {
            warn!("gate: could not save the pairing codes: {e}");
        }
    }

    fn live_records(&self) -> Vec<Record> {
        self.records().into_iter().filter(Record::live).collect()
    }

    /// A new pairing code carrying `security { new-device }`.
    pub fn mint_pair(&self) -> Minted {
        self.mint_pair_with(&Grants::new_device())
    }

    /// A pairing code that, redeemed, gives its device `grants` (on top of any it has). The QR
    /// and the typed code are shown only on this computer, so a device presenting either was
    /// shown the host's screen.
    pub fn mint_pair_with(&self, grants: &Grants) -> Minted {
        use rand::Rng;
        let code: String = rand::thread_rng()
            .sample_iter(&rand::distributions::Alphanumeric)
            .take(24)
            .map(char::from)
            .collect();
        let mut live = self.live_records();
        // Strikes count against the codes they could have hit: with none left, start over.
        if live.is_empty() {
            let _ = fs::remove_file(self.strikes_path());
        }
        let _ = fs::remove_file(self.lockout_path());
        let minted = Minted {
            code,
            typed: typed_code::new(),
            until: now_s() + PAIR_TTL.as_secs(),
        };
        live.push(Record {
            code: minted.code.clone(),
            until: minted.until,
            grants: grants.tokens(),
            typed: minted.typed.clone(),
        });
        self.write_records(&live);
        minted
    }

    /// Spend a pairing code, given in either form: the grants it carries, or `None` if it is
    /// not live. A typed code must already be normalised.
    pub fn redeem(&self, code: &str) -> Option<Grants> {
        let live = self.live_records();
        let hit = live.iter().find(|r| r.is(code))?;
        let g = Grants::parse(&hit.grants).unwrap_or(Grants::NONE);
        let rest: Vec<Record> = live.into_iter().filter(|r| !r.is(code)).collect();
        self.write_records(&rest);
        Some(g)
    }

    /// Whether pairing code `code` (either form) can still be redeemed — false once used,
    /// expired or revoked.
    pub fn pair_live(&self, code: &str) -> bool {
        self.live_records().iter().any(|r| r.is(code))
    }

    /// Withdraw pairing code `code` unused.
    pub fn revoke_pair(&self, code: &str) {
        let _ = self.redeem(code);
    }

    /// The long codes still live, for checking a QR proof against.
    pub fn live_pairs(&self) -> Vec<String> {
        self.live_records().into_iter().map(|r| r.code).collect()
    }

    /// The typed codes still live, for checking a typed proof against.
    pub fn live_typed(&self) -> Vec<String> {
        self.live_records()
            .into_iter()
            .map(|r| r.typed)
            .filter(|t| !t.is_empty())
            .collect()
    }

    /// Count one wrong typed code. The [`MAX_STRIKES`]th revokes every live code, and `true`
    /// says so.
    ///
    /// One appended line per strike, so daemons of a pool striking at once cannot lose a count.
    pub fn strike(&self) -> bool {
        let appended = fs::create_dir_all(&self.dir).and_then(|_| {
            fs::OpenOptions::new()
                .create(true)
                .append(true)
                .open(self.strikes_path())
                .and_then(|mut f| f.write_all(b"x\n"))
        });
        // Fail closed: a count that cannot be kept is a limit that cannot be enforced.
        let strikes = match appended {
            Err(e) => {
                warn!("gate: could not count a wrong pairing code ({e}) — revoking every code");
                MAX_STRIKES
            }
            Ok(()) => fs::read_to_string(self.strikes_path())
                .map(|s| s.lines().count())
                .unwrap_or(MAX_STRIKES),
        };
        if strikes < MAX_STRIKES {
            warn!("gate: a wrong pairing code was typed ({strikes} of {MAX_STRIKES})");
            return false;
        }
        self.write_records(&[]);
        let _ = fs::remove_file(self.strikes_path());
        let _ = fs::write(self.lockout_path(), now_s().to_string());
        warn!(
            "gate: {MAX_STRIKES} wrong pairing codes — every live code is revoked; make a new one \
             with `wado pair`"
        );
        true
    }

    /// Too many wrong codes revoked everything, and no new code has been made since.
    pub fn locked_out(&self) -> bool {
        self.lockout_path().exists()
    }

    /// What became of a code shown on screen, for the screen to say.
    pub fn state(&self, m: &Minted) -> CodeState {
        let now = now_s();
        if self.pair_live(&m.code) {
            CodeState::Live(m.until.saturating_sub(now))
        } else if self.locked_out() {
            CodeState::LockedOut
        } else if now >= m.until {
            CodeState::Expired
        } else {
            CodeState::Used
        }
    }
}

/// What became of a code shown on screen.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum CodeState {
    /// Still works, for this many seconds.
    Live(u64),
    /// A device paired with it (or it was withdrawn).
    Used,
    Expired,
    /// Too many wrong typed codes revoked it.
    LockedOut,
}

impl CodeState {
    /// One line for under the code.
    pub fn say(self) -> String {
        match self {
            Self::Live(s) => format!("expires in {}:{:02}", s / 60, s % 60),
            Self::Used => "used — the device is paired".into(),
            Self::Expired => "expired — make a new one".into(),
            Self::LockedOut => "too many wrong codes — make a new one".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn temp() -> Gate {
        Gate::at(std::env::temp_dir().join(format!("wado-pairing-{}", rand::random::<u64>())))
    }

    /// Rewrite every record's expiry, as if `secs` had passed.
    fn age(g: &Gate, secs: u64) {
        let rs: Vec<Record> = g
            .records()
            .into_iter()
            .map(|r| Record {
                until: r.until - secs,
                ..r
            })
            .collect();
        g.write_records(&rs);
    }

    #[test]
    fn a_code_lives_ten_minutes() {
        assert_eq!(PAIR_TTL, Duration::from_secs(600));
        let g = temp();
        let m = g.mint_pair();
        assert!(m.until <= now_s() + 600 && m.until > now_s() + 590);
        assert!(g.pair_live(&m.code) && g.pair_live(&m.typed));
        age(&g, 600);
        assert!(!g.pair_live(&m.code), "expired");
        assert!(g.redeem(&m.code).is_none());
        assert!(g.redeem(&m.typed).is_none());
    }

    #[test]
    fn the_qr_code_pairs_exactly_once_and_spends_the_typed_one() {
        let g = temp();
        let m = g.mint_pair();
        assert!(g.redeem(&m.code).is_some());
        assert!(g.redeem(&m.code).is_none(), "single use");
        assert!(g.redeem(&m.typed).is_none(), "one record, two forms");
    }

    #[test]
    fn the_typed_code_pairs_exactly_once_and_spends_the_qr_one() {
        let g = temp();
        let m = g.mint_pair();
        assert_eq!(
            typed_code::normalize(&typed_code::display(&m.typed)),
            Some(m.typed.clone())
        );
        assert!(g.live_typed().contains(&m.typed));
        assert!(
            !g.live_pairs().contains(&m.typed),
            "the QR list holds long codes only"
        );
        assert!(g.redeem(&m.typed).is_some());
        assert!(g.redeem(&m.typed).is_none(), "single use");
        assert!(g.redeem(&m.code).is_none(), "one record, two forms");
        assert!(g.redeem("").is_none());
    }

    #[test]
    fn five_wrong_typed_codes_revoke_every_live_code() {
        let g = temp();
        let a = g.mint_pair();
        let b = g.mint_pair();
        for _ in 0..MAX_STRIKES - 1 {
            assert!(!g.strike());
        }
        assert!(
            g.pair_live(&a.typed) && g.pair_live(&b.code),
            "four is not enough"
        );
        assert!(!g.locked_out());
        assert!(g.strike(), "the fifth revokes");
        assert!(!g.pair_live(&a.code) && !g.pair_live(&a.typed));
        assert!(!g.pair_live(&b.code) && !g.pair_live(&b.typed));
        assert!(g.locked_out());
        // A new code is the way out, and it starts with a clean count.
        let c = g.mint_pair();
        assert!(!g.locked_out());
        assert!(!g.strike());
        assert!(g.pair_live(&c.typed));
    }

    #[test]
    fn strikes_are_per_computer_and_survive_a_mint_while_codes_live() {
        let g = temp();
        g.mint_pair();
        for _ in 0..3 {
            g.strike();
        }
        // Another daemon of the pool, or a second code on screen: the same count.
        let other = Gate::at(g.dir.clone());
        let m = other.mint_pair();
        other.strike();
        assert!(g.strike(), "3 + 2 across two handles and two codes");
        assert!(!g.pair_live(&m.typed));
    }

    #[test]
    fn the_screen_is_told_what_became_of_its_code() {
        let g = temp();
        let m = g.mint_pair();
        assert!(matches!(g.state(&m), CodeState::Live(s) if s > 590));
        assert_eq!(CodeState::Live(599).say(), "expires in 9:59");
        g.redeem(&m.typed);
        assert_eq!(g.state(&m), CodeState::Used);
        let m = g.mint_pair();
        age(&g, 600);
        let m = Minted {
            until: m.until - 600,
            ..m
        };
        assert_eq!(g.state(&m), CodeState::Expired);
        let m = g.mint_pair();
        for _ in 0..MAX_STRIKES {
            g.strike();
        }
        assert_eq!(g.state(&m), CodeState::LockedOut);
        assert_eq!(g.state(&m).say(), "too many wrong codes — make a new one");
    }

    #[test]
    fn a_code_from_before_typed_codes_still_works_by_qr() {
        let g = temp();
        fs::create_dir_all(&g.dir).unwrap();
        fs::write(g.pairs_path(), format!("OLDCODE\t{}\thost\n", now_s() + 60)).unwrap();
        assert!(g.live_typed().is_empty());
        assert!(g.redeem("").is_none());
        assert!(g.redeem("OLDCODE").is_some());
    }
}
