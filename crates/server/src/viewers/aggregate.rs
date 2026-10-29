//! One encode, several viewers (spec §D3, E2): the per-viewer signals the compositor used to get
//! from its one viewer, folded into the one answer it still expects.
//!
//! | signal | rule |
//! |---|---|
//! | attached | **any** participant's peer connection is up |
//! | visible | **any** participant is looking |
//! | strain | **any** participant's decoder is saturated (and the log names who) |
//! | bitrate | the **lowest** any participant can take (the user's decision), never below the floor |
//! | keyframe | coalesced: at most one per [`IDR_GAP_MS`] — a join forces one for everyone |
//!
//! Pure: no clock, no compositor. The caller passes `now` and sends what changed.

use std::collections::BTreeMap;

pub use wado_protocol::multi::FLOOR_KBPS;

use crate::session::membership::Origin;

/// At most one forced keyframe per session this often.
pub const IDR_GAP_MS: u64 = 500;

#[derive(Debug, Clone, Default)]
struct Signals {
    attached: bool,
    visible: bool,
    strained: bool,
    /// The bitrate this viewer asked for (its auto-bitrate cap or its own setting), kbps.
    cap_kbps: Option<u32>,
    name: String,
}

#[derive(Debug, Default)]
pub struct Aggregate {
    viewers: BTreeMap<Origin, Signals>,
    last_idr_ms: Option<u64>,
    /// An IDR asked for inside the gap: sent when the gap ends.
    idr_pending: bool,
}

/// The stream's bitrate for everyone, and who is holding it down.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Rate {
    pub kbps: u32,
    /// The viewer whose cap set it, when it is below what the lead asked for.
    pub slowed_by: Option<String>,
    /// That viewer wanted less than the floor: the floor holds, it gets what it gets.
    pub floored: bool,
}

impl Aggregate {
    /// A new viewer counts as looking until it says otherwise — what the compositor assumes of
    /// its one viewer too.
    pub fn add(&mut self, origin: Origin, name: &str) {
        let s = self.viewers.entry(origin).or_insert_with(|| Signals {
            visible: true,
            ..Default::default()
        });
        s.name = name.to_string();
    }

    pub fn remove(&mut self, origin: Origin) {
        self.viewers.remove(&origin);
    }

    pub fn clear(&mut self) {
        self.viewers.clear();
    }

    pub fn has(&self, origin: Origin) -> bool {
        self.viewers.contains_key(&origin)
    }

    /// Only a viewer [`Aggregate::add`]ed counts: a late signal from one already removed must
    /// not bring it back.
    fn get(&mut self, origin: Origin) -> Option<&mut Signals> {
        self.viewers.get_mut(&origin)
    }

    pub fn set_attached(&mut self, origin: Origin, on: bool) {
        if let Some(s) = self.get(origin) {
            s.attached = on;
        }
    }
    pub fn set_visible(&mut self, origin: Origin, on: bool) {
        if let Some(s) = self.get(origin) {
            s.visible = on;
        }
    }
    pub fn set_strained(&mut self, origin: Origin, on: bool) {
        if let Some(s) = self.get(origin) {
            s.strained = on;
        }
    }
    pub fn set_cap(&mut self, origin: Origin, kbps: Option<u32>) {
        if let Some(s) = self.get(origin) {
            s.cap_kbps = kbps;
        }
    }

    pub fn attached(&self) -> bool {
        self.viewers.values().any(|s| s.attached)
    }
    pub fn visible(&self) -> bool {
        self.viewers.values().any(|s| s.visible)
    }
    /// The first strained viewer's name, if any is.
    pub fn strained(&self) -> Option<&str> {
        self.viewers
            .values()
            .find(|s| s.strained)
            .map(|s| s.name.as_str())
    }

    pub fn len(&self) -> usize {
        self.viewers.len()
    }
    pub fn is_empty(&self) -> bool {
        self.viewers.is_empty()
    }

    /// The bitrate for everyone: `lead_kbps` (what the lead's session asks for), lowered to the
    /// smallest cap any participant has, never below the floor — unless the lead itself asked for
    /// less, which is its own choice.
    pub fn rate(&self, lead_kbps: u32) -> Rate {
        let lowest = self
            .viewers
            .values()
            .filter_map(|s| s.cap_kbps.map(|c| (c, s.name.as_str())))
            .min_by_key(|(c, _)| *c);
        match lowest {
            Some((c, who)) if c < lead_kbps => {
                let floor = FLOOR_KBPS.min(lead_kbps);
                Rate {
                    kbps: c.max(floor),
                    slowed_by: Some(who.to_string()),
                    floored: c < floor,
                }
            }
            _ => Rate {
                kbps: lead_kbps,
                slowed_by: None,
                floored: false,
            },
        }
    }

    /// A keyframe is wanted. `true`: force one now. `false`: one went out less than the gap
    /// ago — it is held, and [`Aggregate::idr_due`] releases it.
    pub fn want_idr(&mut self, now: u64) -> bool {
        match self.last_idr_ms {
            Some(t) if now.saturating_sub(t) < IDR_GAP_MS => {
                self.idr_pending = true;
                false
            }
            _ => {
                self.last_idr_ms = Some(now);
                self.idr_pending = false;
                true
            }
        }
    }

    /// A held keyframe whose gap has passed: force it now.
    pub fn idr_due(&mut self, now: u64) -> bool {
        if self.idr_pending
            && self
                .last_idr_ms
                .is_none_or(|t| now.saturating_sub(t) >= IDR_GAP_MS)
        {
            self.idr_pending = false;
            self.last_idr_ms = Some(now);
            return true;
        }
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn any_viewer_attached_or_visible_counts() {
        let mut a = Aggregate::default();
        a.add(1, "A");
        a.add(2, "B");
        assert!(!a.attached() && a.visible(), "looking until told otherwise");
        a.set_visible(1, false);
        a.set_visible(2, false);
        assert!(!a.visible());
        a.set_attached(2, true);
        a.set_visible(1, true);
        assert!(a.attached() && a.visible());
        a.remove(2);
        assert!(!a.attached());
        a.set_attached(2, true);
        assert!(
            !a.attached(),
            "a removed viewer's late signal does not bring it back"
        );
    }

    #[test]
    fn strain_names_who() {
        let mut a = Aggregate::default();
        a.add(1, "A");
        a.add(2, "Pixel");
        assert_eq!(a.strained(), None);
        a.set_strained(2, true);
        assert_eq!(a.strained(), Some("Pixel"));
    }

    #[test]
    fn the_lowest_viewer_sets_the_rate_above_the_floor() {
        let mut a = Aggregate::default();
        a.add(1, "Lead");
        a.add(2, "Pixel");
        assert_eq!(a.rate(8000).kbps, 8000, "no caps: the lead's rate");
        a.set_cap(2, Some(3000));
        assert_eq!(
            a.rate(8000),
            Rate {
                kbps: 3000,
                slowed_by: Some("Pixel".into()),
                floored: false
            }
        );
        a.set_cap(2, Some(600));
        let r = a.rate(8000);
        assert_eq!((r.kbps, r.floored), (FLOOR_KBPS, true), "the floor holds");
        // The lead asking for less than the floor is its own choice.
        assert_eq!(a.rate(1000).kbps, 1000);
        a.remove(2);
        assert_eq!(a.rate(8000).kbps, 8000, "the slow one left: back up");
    }

    #[test]
    fn keyframes_are_coalesced_to_one_per_gap() {
        let mut a = Aggregate::default();
        assert!(a.want_idr(1000));
        assert!(!a.want_idr(1100), "held");
        assert!(!a.want_idr(1300), "still one held, not two");
        assert!(!a.idr_due(1400));
        assert!(a.idr_due(1500), "released at the gap");
        assert!(!a.idr_due(1600), "only once");
        assert!(a.want_idr(2100));
    }
}
