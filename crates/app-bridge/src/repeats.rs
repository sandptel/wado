//! Host-state polls that say nothing new, caught on the event thread. The app polls the computer's
//! state every 1.2–3 s; most answers repeat the last byte for byte. Handed on whole, each was
//! kilobytes to decode and compare on the app's main thread — the thread its video frames wait
//! on. A repeat goes on as `{"type":"host_state","same":true}`, so the app still counts the
//! answer (its polls are paced by them) without reading it.

use std::collections::HashMap;

#[derive(Default)]
pub(crate) struct Repeats {
    last: HashMap<String, String>,
}

#[derive(serde::Deserialize)]
struct Kind<'a> {
    #[serde(borrow, rename = "type")]
    kind: &'a str,
}

pub(crate) const SAME: &str = r#"{"type":"host_state","same":true}"#;

impl Repeats {
    /// True when `json` is a `host_state` identical to the last one room `id` delivered.
    pub(crate) fn same(&mut self, id: &str, json: &str) -> bool {
        if !serde_json::from_str::<Kind>(json).is_ok_and(|k| k.kind == "host_state") {
            return false;
        }
        if self.last.get(id).is_some_and(|l| l == json) {
            return true;
        }
        self.last.insert(id.to_string(), json.to_string());
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn only_a_repeated_host_state_is_same() {
        let mut r = Repeats::default();
        let a = r#"{"type":"host_state","state":{"audio":{}}}"#;
        let b = r#"{"type":"host_state","state":{"audio":{"x":1}}}"#;
        assert!(!r.same("r1", a));
        assert!(r.same("r1", a));
        assert!(!r.same("r2", a), "per room");
        assert!(!r.same("r1", b));
        assert!(!r.same("r1", a), "changed back");
        let s = r#"{"type":"sessions","sessions":[]}"#;
        assert!(
            !r.same("r1", s) && !r.same("r1", s),
            "other messages always pass"
        );
    }
}
