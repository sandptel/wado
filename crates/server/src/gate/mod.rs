//! Who may join this computer — the trust list, and pairing a new device onto it.
//!
//! The relay holds every join until this daemon answers `peer_accept` or `peer_reject`
//! (`WADO_PLAN.md`, Decision Log `2026-10-02`). The rule, decided by the user (`2026-10-27`):
//!
//! - a device whose `client_key` is on the trust list gets straight in;
//! - any other device gets in **only with a pairing code shown on this computer** — the QR's, or
//!   the same code typed ([`pairing`]). Nothing else: no first-device-wins, no approval from
//!   another device. Without a code it is refused with [`UNPAIRED`], which tells it how to pair.
//!
//! Every daemon of a pool runs on one machine and shares [`crate::remote_id::config_dir`], so
//! the trust list and the codes are files there that every daemon reads.
//!
//! ```text
//!   <config>/wado/trusted_clients        <client_key>\t<name>[\t<device pk>\t<pinned>[\t<files>]]
//!   <config>/wado/pair_codes             see `pairing`
//! ```
//!
//! **Since the envelope (`crate::e2e`, `2026-10-04`)** the relay's `client_key` is only a claim:
//! [`Gate::knows`] uses it to let a join through to the envelope, and [`Gate::admit`] makes the
//! decision that counts, once the device has proven its key. A line without a key (older than
//! the envelope) takes the first key proven for it; `pinned` is 1 when that key was proven with
//! a pairing code, which file access requires. `files` is that device's grant — `none` (the
//! default, also when absent), `ro` or `rw` — set by `wado files grant`.

use std::fs;
use std::path::PathBuf;
use std::time::SystemTime;

pub mod confirm;
pub mod grants;
pub mod pairing;
pub mod typed_code;
pub use confirm::{CONFIRM_WAIT, Check};
pub use grants::Grants;
pub use pairing::{CodeState, Minted, PAIR_TTL};
use tracing::{info, warn};

/// Refusal for a device this computer does not know and that brought no code. Shown on the
/// device as is, which also shows it the pairing form — `js/pair.js` matches this wording, and
/// `scripts/pair-check.mjs` fails if the two drift apart.
pub const UNPAIRED: &str = "This computer doesn't know this device yet — on the computer run \
    `wado pair` (or open `wado tui` → Pair) and scan the code or type it here.";
/// Refusal for a typed code that matched no live one.
pub const WRONG_CODE: &str = "That code didn't work — check it, or make a new one on the \
    computer with `wado pair` (or `wado tui` → Pair).";
/// Refusal for a QR code that was already used or has run out.
pub const STALE_CODE: &str = "That pairing code was already used or has expired — make a new \
    one on the computer with `wado pair` (or `wado tui` → Pair).";
/// Refusal for a typed pairing whose check words someone said did not match, on the computer.
pub const DECLINED: &str = "The computer said the check words didn't match, so this device \
    wasn't paired — make a new code on the computer with `wado pair` and try again.";
/// Refusal for a typed pairing nobody confirmed on the computer in time.
pub const NO_ANSWER: &str = "Nobody confirmed the check words on the computer within two \
    minutes, so this device wasn't paired — make a new code with `wado pair` and try again.";
/// Refusal for an unknown device asking to take a desktop someone is using.
pub const BUSY_UNPAIRED: &str = "Every desktop on this computer is in use, and this device isn't \
    paired yet — pair it when one is free.";

fn now_s() -> u64 {
    SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .map_or(0, |d| d.as_secs())
}

/// What a device proved about a pairing code in the envelope.
#[derive(Debug, Clone, PartialEq)]
pub enum Proof {
    /// It sent no code.
    None,
    /// A live QR code — not yet spent.
    Qr(String),
    /// A live typed code — not yet spent. Pairs only once a person confirms the check words.
    Typed(String),
    /// A QR code that is not live: used, expired or revoked.
    Stale,
    /// A typed code that matched nothing live — a guess, as far as the limit is concerned.
    Wrong,
}

/// What [`Gate::admit`] let through.
#[derive(Debug, Clone, PartialEq)]
pub enum Admitted {
    /// In: trusted, or just paired by QR.
    In,
    /// A typed code checked out and is spent, but the device is **not** trusted yet: the check
    /// words must be confirmed on the computer first (`confirm`), then [`Gate::confirmed`] pins
    /// it with these grants.
    Confirm(Grants),
}

/// One line of `trusted_clients`, read.
#[derive(Debug, Clone, PartialEq)]
pub struct Device {
    pub key: String,
    pub name: String,
    /// Its key was proven with a pairing code.
    pub pinned: bool,
    pub grants: Grants,
}

/// The trust list and the pairing codes of one machine.
#[derive(Clone)]
pub struct Gate {
    dir: PathBuf,
}

impl Default for Gate {
    fn default() -> Self {
        Self::at(crate::remote_id::config_dir())
    }
}

impl Gate {
    pub fn at(dir: PathBuf) -> Self {
        Self { dir }
    }

    fn trusted_path(&self) -> PathBuf {
        self.dir.join("trusted_clients")
    }

    /// `(key, name)` for every trusted device, oldest first.
    pub fn trusted(&self) -> Vec<(String, String)> {
        fs::read_to_string(self.trusted_path())
            .unwrap_or_default()
            .lines()
            .filter_map(|l| {
                let mut it = l.split('\t');
                let key = it.next()?.trim();
                (!key.is_empty())
                    .then(|| (key.to_string(), it.next().unwrap_or("").trim().to_string()))
            })
            .collect()
    }

    /// The first device ever trusted — the owner unless `security { owner }` names another.
    pub fn first_trusted(&self) -> Option<String> {
        self.trusted().into_iter().next().map(|(k, _)| k)
    }

    fn trusted_keys(&self) -> Vec<String> {
        fs::read_to_string(self.trusted_path())
            .unwrap_or_default()
            .lines()
            .filter_map(|l| l.split('\t').next())
            .map(str::trim)
            .filter(|k| !k.is_empty())
            .map(String::from)
            .collect()
    }

    /// Is this device on the trust list? Only a claim until [`Gate::admit`] — the relay names
    /// the key. A client with no key is never known: an empty key accepted as trusted would be
    /// a way around the gate for anyone who simply leaves it off.
    pub fn knows(&self, key: &str) -> bool {
        !key.is_empty() && self.trusted_keys().iter().any(|k| k == key)
    }

    /// Add a device to the trust list, unpinned. Pairing is the only way in, so outside a test
    /// a line is written by [`Gate::admit`].
    #[cfg(test)]
    pub fn trust(&self, key: &str, name: &str) {
        if key.is_empty() || self.knows(key) {
            return;
        }
        self.write_line(key, name, "", false, &Grants::new_device());
    }

    /// `(device pk, pinned)` on file for `key`; the pk is empty for a line from before keys.
    pub fn entry(&self, key: &str) -> Option<(String, bool)> {
        fs::read_to_string(self.trusted_path())
            .unwrap_or_default()
            .lines()
            .map(|l| l.split('\t').collect::<Vec<_>>())
            .find(|f| f[0].trim() == key)
            .map(|f| {
                let pk = f.get(2).map_or("", |s| s.trim()).to_string();
                (pk, f.get(3).is_some_and(|s| s.trim() == "1"))
            })
    }

    /// Write `key`'s line with its proven key, in place or appended. Its grants are kept, plus
    /// `more` (a pairing code's checklist); a new line starts with `security { new-device }`.
    fn set(&self, key: &str, name: &str, pk: &str, pinned: bool, more: Option<&Grants>) {
        let had = self
            .device(key)
            .map_or_else(Grants::new_device, |d| d.grants);
        let g = more.map_or_else(|| had.clone(), |m| had.union(m));
        self.write_line(key, name, pk, pinned, &g);
    }

    fn write_line(&self, key: &str, name: &str, pk: &str, pinned: bool, g: &Grants) {
        let name = name.replace(['\t', '\n'], " ");
        let line = format!(
            "{key}\t{name}\t{pk}\t{}\t{}\t{}",
            u8::from(pinned),
            g.files,
            g.flags()
        );
        let mut lines: Vec<String> = fs::read_to_string(self.trusted_path())
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty())
            .map(String::from)
            .collect();
        match lines.iter_mut().find(|l| l.split('\t').next() == Some(key)) {
            Some(l) => *l = line,
            None => lines.push(line),
        }
        if let Err(e) = fs::create_dir_all(&self.dir)
            .and_then(|_| fs::write(self.trusted_path(), lines.join("\n") + "\n"))
        {
            warn!("gate: could not write the trust list: {e}");
        }
    }

    /// Every trusted device, oldest first.
    pub fn devices(&self) -> Vec<Device> {
        fs::read_to_string(self.trusted_path())
            .unwrap_or_default()
            .lines()
            .map(|l| l.split('\t').map(str::trim).collect::<Vec<_>>())
            .filter(|f| !f[0].is_empty())
            .map(|f| {
                let get = |i: usize| f.get(i).copied().unwrap_or("");
                Device {
                    key: get(0).into(),
                    name: get(1).into(),
                    pinned: get(3) == "1",
                    grants: Grants::from_columns(get(4), f.get(5).copied()),
                }
            })
            .collect()
    }

    pub fn device(&self, key: &str) -> Option<Device> {
        self.devices().into_iter().find(|d| d.key == key)
    }

    /// What `key` may do: its line's grants; the owner may do everything but files without
    /// asking; a device off the list, nothing.
    pub fn grants(&self, key: &str) -> Grants {
        let own = self.device(key).map_or(Grants::NONE, |d| d.grants);
        if crate::config::link::is_owner(key, self) {
            // Files stay an explicit grant, owner or not.
            return Grants {
                files: own.files,
                ..Grants::ALL
            };
        }
        own
    }

    /// What `key` may do with files: `none`, `ro` or `rw`. A grant counts only on a line whose
    /// key was proven with a pairing code — a legacy line gets nothing until it is re-paired.
    pub fn files_access(&self, key: &str) -> &'static str {
        match self.device(key) {
            Some(d) if d.pinned => self.grants(key).files,
            _ => "none",
        }
    }

    /// Set a device's file grant. `who` is its key, a prefix of it, or its exact name; it must
    /// name exactly one device. Returns the device's name.
    pub fn grant(&self, who: &str, level: &str) -> Result<String, String> {
        if !matches!(level, "none" | "ro" | "rw") {
            return Err(format!("`{level}` is not a level — use none, ro or rw"));
        }
        let token = if level == "none" {
            "files"
        } else {
            &format!("files-{level}")
        };
        self.allow(who, &[token], level != "none")
            .map(|(name, _)| name)
    }

    /// Turn grants on or off for the device `who` names. Returns its name and grants now.
    pub fn allow(&self, who: &str, tokens: &[&str], on: bool) -> Result<(String, Grants), String> {
        let d = self.find(who)?;
        let mut g = d.grants.clone();
        for t in tokens {
            g.set(t, on)?;
        }
        let pk = self.entry(&d.key).map(|e| e.0).unwrap_or_default();
        self.write_line(&d.key, &d.name, &pk, d.pinned, &g);
        info!("gate: {} may now: {}", d.name, g.tokens());
        Ok((d.name, g))
    }

    /// A device name as it must be typed after `wado allow`: quoted when it has spaces.
    pub fn typed(name: &str) -> String {
        if name.contains(char::is_whitespace) {
            format!("\"{name}\"")
        } else {
            name.to_string()
        }
    }

    /// The one device `who` names: its key, a prefix of it, its exact name, or — when that
    /// names nothing — the start of its name, any case (`android` for "Android · Chrome").
    pub fn find(&self, who: &str) -> Result<Device, String> {
        let all = self.devices();
        let mut hits: Vec<_> = all
            .iter()
            .filter(|d| !who.is_empty() && (d.key.starts_with(who) || d.name == who))
            .collect();
        if hits.is_empty() && !who.is_empty() {
            let low = who.to_lowercase();
            hits = all
                .iter()
                .filter(|d| d.name.to_lowercase().starts_with(&low))
                .collect();
        }
        match hits.as_slice() {
            [one] => Ok((*one).clone()),
            [] => Err(format!("no trusted device matches `{who}`")),
            _ => Err(format!(
                "`{who}` matches {} devices — use more of the key",
                hits.len()
            )),
        }
    }

    /// Forget a device: it must be paired again to get back in. Returns its name.
    pub fn unpair(&self, who: &str) -> Result<String, String> {
        let Device { key, name, .. } = self.find(who)?;
        let kept: String = fs::read_to_string(self.trusted_path())
            .unwrap_or_default()
            .lines()
            .filter(|l| !l.trim().is_empty() && l.split('\t').next() != Some(key.as_str()))
            .map(|l| format!("{l}\n"))
            .collect();
        fs::write(self.trusted_path(), kept).map_err(|e| e.to_string())?;
        info!("gate: unpaired {name}");
        Ok(name)
    }

    /// The decision that counts: may the device that proved `pk` in, as `key`? `proof` is what
    /// it proved about a pairing code. The error is the reason shown to the device.
    pub fn admit(
        &self,
        key: &str,
        name: &str,
        pk: &str,
        proof: &Proof,
    ) -> Result<Admitted, &'static str> {
        // A code is the owner's say-so, and it pins whatever key proved it — a re-pair is how
        // a device that lost its key comes back.
        let unknown = match proof {
            Proof::Qr(c) => match self.redeem(c).map(|g| g.union(&Grants::CODE_PAIRED)) {
                Some(g) => {
                    info!(
                        "gate: {name} paired with a QR code — trusted and pinned, may: {}",
                        g.tokens()
                    );
                    self.set(key, name, pk, true, Some(&g));
                    return Ok(Admitted::In);
                }
                // Spent between the proof and here — by another daemon of the pool, say.
                None => STALE_CODE,
            },
            // Spent now, so it works once whatever the answer; trusted only on a yes.
            Proof::Typed(c) => match self.redeem(c) {
                Some(g) => {
                    info!("gate: {name} proved a typed code — waiting for the check words");
                    return Ok(Admitted::Confirm(g));
                }
                None => STALE_CODE,
            },
            Proof::Stale => STALE_CODE,
            Proof::Wrong => {
                self.strike();
                WRONG_CODE
            }
            Proof::None => UNPAIRED,
        };
        match self.entry(key) {
            Some((on_file, _)) if on_file == pk => Ok(Admitted::In),
            Some((on_file, _)) if !on_file.is_empty() => {
                Err("this device's key is not the one this computer trusts for it")
            }
            Some(_) => {
                info!("gate: {name} proved a key for the first time — remembered (not pinned)");
                self.set(key, name, pk, false, None);
                Ok(Admitted::In)
            }
            None => Err(unknown),
        }
    }

    /// The check words of a typed pairing were confirmed on the computer: trust and pin.
    pub fn confirmed(&self, key: &str, name: &str, pk: &str, grants: &Grants) {
        let grants = &grants.union(&Grants::CODE_PAIRED);
        info!(
            "gate: {name} paired with a typed code, check words confirmed — trusted and pinned, \
             may: {}",
            grants.tokens()
        );
        self.set(key, name, pk, true, Some(grants));
    }
}

#[cfg(test)]
mod tests {
    use super::{Admitted, Gate, Grants, Proof, STALE_CODE, UNPAIRED, WRONG_CODE};

    fn temp() -> Gate {
        let dir = std::env::temp_dir().join(format!("wado-gate-{}", rand::random::<u64>()));
        Gate::at(dir)
    }

    #[test]
    fn a_device_is_found_by_the_start_of_its_name() {
        let g = temp();
        g.trust("k1", "Android · Chrome");
        g.trust("k2", "Linux · Firefox");
        assert_eq!(g.find("android").unwrap().key, "k1");
        assert_eq!(g.find("Android · Chrome").unwrap().key, "k1");
        assert!(g.find("Mac").is_err());
        g.trust("k3", "Android · Firefox");
        assert!(g.find("android").is_err(), "two Androids: ambiguous");
        assert_eq!(Gate::typed("Android · Chrome"), "\"Android · Chrome\"");
        assert_eq!(Gate::typed("pixel"), "pixel");
    }

    #[test]
    fn no_device_gets_in_without_a_code_not_even_the_first() {
        let g = temp();
        assert!(!g.knows("phone"));
        assert!(g.trusted().is_empty(), "nothing trusted on sight");
        assert_eq!(
            g.admit("phone", "Phone", "PK1", &Proof::None),
            Err(UNPAIRED)
        );
        assert!(
            g.trusted().is_empty(),
            "and still nothing after the refusal"
        );
        assert!(!temp().knows(""));
    }

    #[test]
    fn a_pairing_code_lives_ten_minutes() {
        assert_eq!(super::PAIR_TTL, std::time::Duration::from_secs(600));
    }

    #[test]
    fn a_qr_code_pairs_exactly_once_and_at_once() {
        let g = temp();
        let qr = g.mint_pair();
        assert_eq!(
            g.admit("phone", "Phone", "PK1", &Proof::Qr(qr.code.clone())),
            Ok(Admitted::In)
        );
        assert!(g.knows("phone"));
        assert_eq!(g.entry("phone"), Some(("PK1".into(), true)), "pinned");
        // Used: the next device holding the same code gets nothing.
        assert_eq!(
            g.admit("tab", "Tab", "PK2", &Proof::Qr(qr.code.clone())),
            Err(STALE_CODE)
        );
    }

    #[test]
    fn a_typed_code_is_spent_once_and_pins_nothing_until_confirmed() {
        let g = temp();
        let m = g.mint_pair_with(&Grants::parse("shells").unwrap());
        let got = g.admit("laptop", "Laptop", "PK3", &Proof::Typed(m.typed.clone()));
        assert_eq!(got, Ok(Admitted::Confirm(Grants::parse("shells").unwrap())));
        assert!(
            !g.knows("laptop"),
            "not trusted before the words are confirmed"
        );
        assert_eq!(g.entry("laptop"), None, "not pinned either");
        assert!(
            !g.pair_live(&m.typed) && !g.pair_live(&m.code),
            "spent all the same"
        );
        assert_eq!(
            g.admit("tab", "Tab", "PK2", &Proof::Typed(m.typed)),
            Err(STALE_CODE),
            "single use, even while the first waits"
        );
        g.confirmed("laptop", "Laptop", "PK3", &Grants::parse("shells").unwrap());
        assert_eq!(
            g.entry("laptop"),
            Some(("PK3".into(), true)),
            "pinned on a yes"
        );
        assert!(g.device("laptop").unwrap().grants.shells);
        assert_eq!(
            g.admit("laptop", "Laptop", "PK3", &Proof::None),
            Ok(Admitted::In)
        );
        assert!(!g.knows("tab"));
    }

    #[test]
    fn five_wrong_typed_codes_through_admit_revoke_every_live_code() {
        let g = temp();
        let m = g.mint_pair();
        for _ in 0..4 {
            assert_eq!(g.admit("x", "X", "PKX", &Proof::Wrong), Err(WRONG_CODE));
        }
        assert!(g.pair_live(&m.typed));
        assert_eq!(g.admit("y", "Y", "PKY", &Proof::Wrong), Err(WRONG_CODE));
        assert!(!g.pair_live(&m.typed) && !g.pair_live(&m.code), "revoked");
        assert!(g.locked_out());
        assert_eq!(
            g.admit("z", "Z", "PKZ", &Proof::Qr(m.code)),
            Err(STALE_CODE),
            "the real code is gone too"
        );
    }

    #[test]
    fn a_trusted_device_keeps_working_untouched() {
        let g = temp();
        let m = g.mint_pair();
        g.admit("phone", "Phone", "PK1", &Proof::Qr(m.code))
            .unwrap();
        let line = std::fs::read_to_string(g.trusted_path()).unwrap();
        for _ in 0..3 {
            assert_eq!(
                g.admit("phone", "Phone", "PK1", &Proof::None),
                Ok(Admitted::In)
            );
        }
        assert_eq!(
            std::fs::read_to_string(g.trusted_path()).unwrap(),
            line,
            "its line is not rewritten"
        );
        // A line from before this change — no key, no pin — is still let in, and still takes
        // the first key proven for it.
        g.trust("legacy", "Old phone");
        assert!(g.knows("legacy"));
        assert_eq!(
            g.admit("legacy", "Old phone", "PKL", &Proof::None),
            Ok(Admitted::In)
        );
        assert_eq!(g.entry("legacy"), Some(("PKL".into(), false)));
    }

    #[test]
    fn the_proven_key_decides_not_the_claimed_id() {
        let g = temp();
        // A line from before keys takes the first key proven for it…
        g.trust("phone", "Phone");
        assert!(g.admit("phone", "Phone", "PK1", &Proof::None).is_ok());
        assert!(g.admit("phone", "Phone", "PK1", &Proof::None).is_ok());
        // …and after that a relay naming "phone" with any other key gets nothing.
        assert!(g.admit("phone", "Phone", "EVIL", &Proof::None).is_err());
        // Unknown: refused, even though the relay let it in on a pair claim.
        assert!(g.admit("laptop", "Laptop", "PK2", &Proof::None).is_err());
        assert_eq!(g.entry("laptop"), None);
        // A proven code trusts and pins, and replaces a lost key.
        let c = g.mint_pair().code;
        assert!(
            g.admit("phone", "Phone", "PK3", &Proof::Qr(c.clone()))
                .is_ok()
        );
        assert_eq!(g.entry("phone"), Some(("PK3".into(), true)));
        assert!(
            g.admit("phone", "Phone", "PK1", &Proof::Qr(c)).is_err(),
            "single use"
        );
        assert_eq!(g.trusted().len(), 1, "rewritten in place, not appended");
    }

    #[test]
    fn file_access_needs_a_grant_and_a_code_pinned_key() {
        let g = temp();
        g.trust("phone", "Phone");
        assert!(g.admit("phone", "Phone", "PK1", &Proof::None).is_ok());
        assert_eq!(g.files_access("phone"), "none", "no grant yet");
        assert_eq!(g.grant("pho", "rw").unwrap(), "Phone");
        assert_eq!(g.files_access("phone"), "none", "a legacy line, not pinned");
        let c = g.mint_pair().code;
        assert!(g.admit("phone", "Phone", "PK1", &Proof::Qr(c)).is_ok());
        assert_eq!(g.files_access("phone"), "rw", "a re-pair keeps the grant");
        assert!(g.grant("phone", "root").is_err());
        assert!(g.grant("nobody", "ro").is_err());
        g.trust("phone2", "Phone 2");
        assert!(
            g.grant("phone", "ro").is_err(),
            "a prefix of two keys is ambiguous"
        );
        assert!(g.grant("Phone 2", "ro").is_ok(), "by exact name");
        assert_eq!(g.files_access("laptop"), "none");
    }

    #[test]
    fn unpair_forgets_one_device() {
        let g = temp();
        g.trust("phone", "Phone");
        g.trust("laptop", "Laptop");
        assert_eq!(g.unpair("Phone").unwrap(), "Phone");
        assert!(!g.knows("phone"));
        assert_eq!(g.trusted().len(), 1);
        assert!(g.unpair("phone").is_err(), "already gone");
    }
}
