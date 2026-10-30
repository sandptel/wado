//! Who is in this daemon's session: the participants, the lead, and the rules for joining,
//! taking over and leaving (Decision Log `2026-10-30`; spec §D6).
//!
//! - **Lead**: whoever started the session or last took it over. The output is its size. If it
//!   leaves the role stays with that device — a label and a sizing source, not a lock. A device
//!   joining a session with nobody in it becomes the lead (it displaces nobody).
//! - **Join** needs a free place (the cap, which the speed test may lower). The same device
//!   twice — two tabs, one key — replaces its older self, which is told and does not retry.
//! - **Take over** makes the taker the only participant and the lead. Refused within
//!   [`COOLDOWN_MS`] of the last one, so two people cannot ping-pong a session.
//! - **End** is anyone's (the user's decision): everyone else is dropped.
//!
//! Pure: no clock, no I/O — the caller passes `now`, and acts on what each call returns.

use wado_protocol::multi::Participant;

/// The server's number for a participant's peer connection — the arbiter's `Origin`.
pub type Origin = u16;

/// A take over within this long of the last one is refused.
pub const COOLDOWN_MS: u64 = 10_000;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Member {
    pub origin: Origin,
    /// The device's `client_key` — never sent to another device.
    pub key: String,
    pub name: String,
    pub since_ms: u64,
    /// Its link, as it last reported it (`direct · lan · 4 ms`).
    pub link: String,
}

impl Member {
    pub fn new(origin: Origin, key: &str, name: &str, now: u64) -> Self {
        Self {
            origin,
            key: key.into(),
            name: name.into(),
            since_ms: now,
            link: String::new(),
        }
    }
}

/// Why a join or take over was refused — each says it in words the device shows.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum Refusal {
    Full { max: usize },
    Cooldown { by: String, wait_s: u64 },
}

impl Refusal {
    pub fn message(&self) -> String {
        match self {
            Refusal::Full { max } => format!(
                "this session already has {max} devices in it — the most it can carry. End or \
                 leave it on one of them first."
            ),
            Refusal::Cooldown { by, wait_s } => {
                format!("{by} just took this session over — try again in {wait_s} s")
            }
        }
    }
}

#[derive(Debug, Default)]
pub struct Membership {
    members: Vec<Member>,
    /// `(key, name)` of the lead.
    lead: Option<(String, String)>,
    /// `(when, by)` of the last take over.
    last_takeover: Option<(u64, String)>,
}

impl Membership {
    /// A new session, started by `m`. Anyone still listed (there should be nobody) is returned
    /// to be dropped.
    pub fn start(&mut self, m: Member) -> Vec<Member> {
        self.lead = Some((m.key.clone(), m.name.clone()));
        self.last_takeover = None;
        std::mem::replace(&mut self.members, vec![m])
    }

    /// `m` joins. `Ok(replaced)`: the same device's older connection, to be told
    /// `opened_here_again` and dropped.
    pub fn join(&mut self, m: Member, cap: usize) -> Result<Option<Member>, Refusal> {
        let replaced = self
            .members
            .iter()
            .position(|x| x.key == m.key && !m.key.is_empty())
            .map(|i| self.members.remove(i));
        if replaced.is_none() && self.members.len() >= cap.max(1) {
            return Err(Refusal::Full { max: cap.max(1) });
        }
        if self.members.is_empty() && replaced.is_none() {
            // Nobody in it: the device that comes back is who it is sized for.
            self.lead = Some((m.key.clone(), m.name.clone()));
        }
        self.members.push(m);
        Ok(replaced)
    }

    /// `m` takes the session over: it is the lead and the only participant. Returns everyone
    /// displaced.
    pub fn takeover(&mut self, m: Member, now: u64) -> Result<Vec<Member>, Refusal> {
        if let Some((at, by)) = &self.last_takeover {
            let since = now.saturating_sub(*at);
            let own = self.lead.as_ref().is_some_and(|(k, _)| *k == m.key);
            if since < COOLDOWN_MS && !own {
                return Err(Refusal::Cooldown {
                    by: by.clone(),
                    wait_s: (COOLDOWN_MS - since).div_ceil(1000),
                });
            }
        }
        self.lead = Some((m.key.clone(), m.name.clone()));
        self.last_takeover = Some((now, m.name.clone()));
        let others = std::mem::take(&mut self.members)
            .into_iter()
            .filter(|x| x.origin != m.origin && x.key != m.key)
            .collect();
        self.members = vec![m];
        Ok(others)
    }

    /// `origin` left (or its connection died). The lead label stays.
    pub fn leave(&mut self, origin: Origin) -> Option<Member> {
        let i = self.members.iter().position(|m| m.origin == origin)?;
        Some(self.members.remove(i))
    }

    /// The session ended: everyone goes.
    pub fn end(&mut self) -> Vec<Member> {
        self.lead = None;
        self.last_takeover = None;
        std::mem::take(&mut self.members)
    }

    pub fn has(&self, origin: Origin) -> bool {
        self.members.iter().any(|m| m.origin == origin)
    }

    pub fn get(&self, origin: Origin) -> Option<&Member> {
        self.members.iter().find(|m| m.origin == origin)
    }

    pub fn members(&self) -> &[Member] {
        &self.members
    }

    pub fn len(&self) -> usize {
        self.members.len()
    }

    pub fn is_empty(&self) -> bool {
        self.members.is_empty()
    }

    pub fn lead_name(&self) -> Option<&str> {
        self.lead.as_ref().map(|(_, n)| n.as_str())
    }

    pub fn is_lead(&self, origin: Origin) -> bool {
        match (&self.lead, self.get(origin)) {
            (Some((k, _)), Some(m)) => *k == m.key,
            _ => false,
        }
    }

    /// The lead's origin while it is in the session.
    pub fn lead_origin(&self) -> Option<Origin> {
        let (k, _) = self.lead.as_ref()?;
        self.members.iter().find(|m| m.key == *k).map(|m| m.origin)
    }

    pub fn set_link(&mut self, origin: Origin, link: String) {
        if let Some(m) = self.members.iter_mut().find(|m| m.origin == origin) {
            m.link = link;
        }
    }

    /// The list as every participant sees it: the lead first, then by when they joined.
    pub fn list(&self) -> Vec<Participant> {
        let mut out: Vec<Participant> = self
            .members
            .iter()
            .map(|m| Participant {
                id: m.origin,
                name: m.name.clone(),
                lead: self.lead.as_ref().is_some_and(|(k, _)| *k == m.key),
                since_ms: m.since_ms,
                link: m.link.clone(),
                hue: hue(&m.key),
            })
            .collect();
        out.sort_by_key(|p| (!p.lead, p.since_ms));
        out
    }
}

/// A device's colour, 0..8, from its key — the same on every screen and every session.
pub fn hue(key: &str) -> u8 {
    (key.bytes()
        .fold(2166136261u32, |h, b| (h ^ b as u32).wrapping_mul(16777619))
        % 8) as u8
}

#[cfg(test)]
mod tests {
    use super::*;

    fn m(origin: Origin, key: &str) -> Member {
        Member::new(origin, key, &key.to_uppercase(), origin as u64)
    }

    #[test]
    fn the_starter_leads_and_others_join() {
        let mut s = Membership::default();
        assert!(s.start(m(1, "a")).is_empty());
        assert_eq!(s.join(m(2, "b"), 4), Ok(None));
        assert!(s.is_lead(1) && !s.is_lead(2));
        assert_eq!(s.lead_origin(), Some(1));
        let l = s.list();
        assert_eq!((l[0].name.as_str(), l[0].lead), ("A", true));
        assert_eq!(l.len(), 2);
    }

    #[test]
    fn a_full_session_refuses_and_says_how_many() {
        let mut s = Membership::default();
        s.start(m(1, "a"));
        s.join(m(2, "b"), 2).unwrap();
        assert_eq!(s.join(m(3, "c"), 2), Err(Refusal::Full { max: 2 }));
        assert!(Refusal::Full { max: 2 }.message().contains("2 devices"));
    }

    #[test]
    fn the_same_device_twice_replaces_its_older_self() {
        let mut s = Membership::default();
        s.start(m(1, "a"));
        s.join(m(2, "b"), 2).unwrap();
        // Full — but it is b again, in another tab: it replaces itself, it is not refused.
        let old = s.join(m(3, "b"), 2).unwrap().unwrap();
        assert_eq!(old.origin, 2);
        assert!(s.has(3) && !s.has(2));
    }

    #[test]
    fn a_takeover_leaves_only_the_taker_and_cools_down() {
        let mut s = Membership::default();
        s.start(m(1, "a"));
        s.join(m(2, "b"), 4).unwrap();
        s.join(m(3, "c"), 4).unwrap();
        let gone = s.takeover(m(4, "d"), 100_000).unwrap();
        assert_eq!(gone.iter().map(|g| g.origin).collect::<Vec<_>>(), [1, 2, 3]);
        assert!(s.is_lead(4) && s.len() == 1);
        // Within 10 s, someone else is refused, named.
        match s.takeover(m(5, "e"), 104_000) {
            Err(Refusal::Cooldown { by, wait_s }) => assert_eq!((by.as_str(), wait_s), ("D", 6)),
            other => panic!("{other:?}"),
        }
        assert!(s.takeover(m(5, "e"), 110_000).is_ok());
    }

    #[test]
    fn the_lead_leaving_keeps_the_label_and_a_returner_to_an_empty_session_leads() {
        let mut s = Membership::default();
        s.start(m(1, "a"));
        s.join(m(2, "b"), 4).unwrap();
        s.leave(1);
        assert_eq!(s.lead_name(), Some("A"), "still sized for A");
        assert_eq!(s.lead_origin(), None);
        // A comes back: it leads again.
        s.join(m(3, "a"), 4).unwrap();
        assert!(s.is_lead(3));
        // Everyone leaves; the next to come back leads.
        s.leave(2);
        s.leave(3);
        s.join(m(4, "c"), 4).unwrap();
        assert!(s.is_lead(4));
    }

    #[test]
    fn end_drops_everyone() {
        let mut s = Membership::default();
        s.start(m(1, "a"));
        s.join(m(2, "b"), 4).unwrap();
        assert_eq!(s.end().len(), 2);
        assert!(s.is_empty() && s.lead_name().is_none());
    }

    #[test]
    fn hues_are_stable_and_in_range() {
        assert_eq!(hue("abc"), hue("abc"));
        assert!((0..8).contains(&hue("whatever")));
    }
}
