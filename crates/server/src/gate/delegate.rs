//! A device that manages devices (`2026-11-04`, the `devices` grant): it asks this computer for
//! ordinary pairing codes for other devices and signs out the ones it let in. The guardrails live
//! here, as one rule each:
//!
//! - only a device holding `devices` may ask — and only this computer gives that grant;
//! - the code carries at most what the asking device has, never `devices` ([`Grants::cap`]);
//! - it trusts for at most [`DELEGATE_MAX_S`];
//! - the asking device may answer the check words of its own codes, nobody else's;
//! - it may sign out only devices it let in — never the owner, never a line this computer wrote;
//! - signing it out signs them out (`Gate::unpair` and `Gate::prune` drop the orphans).
//!
//! The passkey step-up before a code is asked for is the caller's (the relay link knows the
//! device's unlock); this module trusts nothing it is told about who is asking beyond the key.

use tracing::info;

use super::confirm::Ask;
use super::grants::DELEGATE_MAX_S;
use super::{Device, Gate, Grants, Minted};

pub const NOT_MANAGER: &str = "this device may not let other devices in — only the computer can \
    allow that: on the computer, wado allow <this device> devices";
pub const NOT_YOURS: &str = "this device didn't let that one in, so it can't sign it out — do it \
    on the computer (wado unpair, or wado tui → Devices)";

impl Gate {
    /// A pairing code asked for by `by`: `want` capped to `by`'s own grants, trusted for
    /// `trust_for` seconds capped to seven days.
    pub fn mint_for(
        &self,
        by: &str,
        want: &Grants,
        trust_for: u64,
    ) -> Result<Minted, &'static str> {
        let mine = self.grants(by);
        if !mine.devices || self.device(by).is_none() {
            return Err(NOT_MANAGER);
        }
        let g = want.cap(&mine);
        let secs = trust_for.clamp(60, DELEGATE_MAX_S);
        info!(
            "gate: {} asked for a pairing code — may: {}, for {secs} s",
            self.device(by).map_or_else(|| by.to_string(), |d| d.name),
            g.tokens()
        );
        Ok(self.mint_pair_for(&g, Some(secs), by))
    }

    /// The devices `by` let in.
    pub fn minted_by(&self, by: &str) -> Vec<Device> {
        self.devices()
            .into_iter()
            .filter(|d| !by.is_empty() && d.term.by == by)
            .collect()
    }

    /// Sign out `key` on `by`'s word: only a device `by` let in.
    pub fn sign_out_as(&self, by: &str, key: &str) -> Result<String, &'static str> {
        if !self.grants(by).devices {
            return Err(NOT_MANAGER);
        }
        match self.device(key) {
            Some(d) if !by.is_empty() && d.term.by == by => {
                self.unpair(&d.key).map_err(|_| NOT_YOURS)
            }
            _ => Err(NOT_YOURS),
        }
    }

    /// The check-word questions of codes `by` asked for, still waiting.
    pub fn asking_for(&self, by: &str) -> Vec<Ask> {
        self.asking()
            .into_iter()
            .filter(|a| !by.is_empty() && a.by == by)
            .collect()
    }

    /// `by` answers the check words of one of its own codes.
    pub fn answer_as(&self, by: &str, typed: &str, yes: bool) -> bool {
        self.grants(by).devices
            && self.asking_for(by).iter().any(|a| a.typed == typed)
            && self.answer(typed, yes)
    }
}

#[cfg(test)]
mod tests {
    use super::super::{Admitted, Gate, Grants, Proof};

    fn temp() -> Gate {
        Gate::at(std::env::temp_dir().join(format!("wado-delegate-{}", rand::random::<u64>())))
    }

    /// A pinned device with `grants`, written as `wado allow` would.
    fn device(g: &Gate, key: &str, grants: &str) {
        let m = g.mint_pair_with(&Grants::parse(grants).unwrap());
        assert_eq!(
            g.admit(key, key, &format!("PK{key}"), &Proof::Qr(m.code)),
            Ok(Admitted::In)
        );
    }

    #[test]
    fn only_a_manager_mints_within_its_own_grants_for_at_most_a_week() {
        let g = temp();
        device(&g, "owner", "clipboard");
        device(&g, "phone", "clipboard,shells,devices");
        g.allow("phone", &["join", "takeover"], false).unwrap();
        device(&g, "plain", "clipboard");
        assert!(
            g.mint_for("plain", &Grants::parse("clipboard").unwrap(), 3600)
                .is_err(),
            "no grant, no code"
        );
        assert!(g.mint_for("stranger", &Grants::NONE, 3600).is_err());
        // Asks for more than it has, and for a month: gets its own, minus devices, for a week.
        let m = g
            .mint_for(
                "phone",
                &Grants::parse("clipboard,files-rw,settings,devices").unwrap(),
                30 * 86_400,
            )
            .unwrap();
        assert_eq!(
            g.admit("tv", "TV", "PKTV", &Proof::Qr(m.code)),
            Ok(Admitted::In)
        );
        let tv = g.device("tv").unwrap();
        assert!(
            tv.grants.clipboard
                && !tv.grants.settings
                && !tv.grants.devices
                && tv.grants.files == "none"
                && !tv.grants.join
                && !tv.grants.takeover,
            "nothing the phone could not give — not even what a code on the computer adds"
        );
        assert_eq!(tv.term.by, "phone");
        let left = tv.term.until.unwrap() - super::super::now_s();
        assert!(left <= super::DELEGATE_MAX_S && left > super::DELEGATE_MAX_S - 60);
        // Being the owner is not the computer's say-so: no devices grant from it.
        assert!(!g.grants("owner").devices);
    }

    #[test]
    fn a_manager_signs_out_only_its_own_and_takes_them_with_it() {
        let g = temp();
        device(&g, "phone", "devices,clipboard");
        device(&g, "laptop", "clipboard");
        let m = g
            .mint_for("phone", &Grants::parse("clipboard").unwrap(), 3600)
            .unwrap();
        g.admit("tv", "TV", "PKTV", &Proof::Qr(m.code)).unwrap();
        assert!(
            g.sign_out_as("phone", "laptop").is_err(),
            "the computer paired that one"
        );
        assert!(
            g.sign_out_as("tv", "tv").is_err(),
            "a delegate has no devices grant"
        );
        let m2 = g.mint_for("phone", &Grants::NONE, 3600).unwrap();
        g.admit("tab", "Tab", "PKTAB", &Proof::Qr(m2.code)).unwrap();
        assert!(g.sign_out_as("phone", "tab").is_ok());
        assert!(!g.knows("tab"));
        // Signing the phone out signs out what it let in.
        g.unpair("phone").unwrap();
        assert!(!g.knows("tv") && g.knows("laptop"));
    }

    #[test]
    fn a_manager_answers_the_check_words_of_its_own_codes_only() {
        let g = temp();
        device(&g, "phone", "devices");
        device(&g, "other", "devices");
        let m = g.mint_for("phone", &Grants::NONE, 3600).unwrap();
        g.ask(&m.typed, "a b c d", "TV", "phone");
        assert_eq!(g.asking_for("phone").len(), 1);
        assert!(g.asking_for("other").is_empty());
        assert!(!g.answer_as("other", &m.typed, true), "not its code");
        assert!(g.answer_as("phone", &m.typed, true));
    }
}
