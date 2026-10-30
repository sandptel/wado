//! Join URLs on the relay (`/join/<rid>?…`, `crates/relay/src/join.rs`), as `js/relay_link.js`
//! builds them:
//!
//! - the **seat** with `pair=1`, for the pairing ceremony only (A9.4);
//! - a **control room**, `control=1&strict=1`: seatless, one per computer, every feature;
//! - a **view room** (P6): a control room whose first sealed message is `room_hello{view}`.
//!
//! `http(s)` becomes `ws(s)`; query values are percent-encoded.

use percent_encoding::{utf8_percent_encode, AsciiSet, NON_ALPHANUMERIC};
use wado_protocol::relay_wire::normalize_remote_id;

/// As JS `encodeURIComponent`: everything but `A-Z a-z 0-9 - _ . ! ~ * ' ( )`.
const COMPONENT: &AsciiSet = &NON_ALPHANUMERIC
    .remove(b'-')
    .remove(b'_')
    .remove(b'.')
    .remove(b'!')
    .remove(b'~')
    .remove(b'*')
    .remove(b'\'')
    .remove(b'(')
    .remove(b')');

fn enc(s: &str) -> String {
    utf8_percent_encode(s, COMPONENT).to_string()
}

fn ws_base(relay: &str) -> String {
    let r = relay.trim();
    let r = if let Some(rest) = r.strip_prefix("https://") {
        format!("wss://{rest}")
    } else if let Some(rest) = r.strip_prefix("http://") {
        format!("ws://{rest}")
    } else {
        r.to_string()
    };
    r.trim_end_matches('/').to_string()
}

fn join(relay: &str, rid: &str, client_key: &str, name: &str) -> String {
    format!(
        "{}/join/{}?client={}&name={}",
        ws_base(relay),
        enc(&normalize_remote_id(rid)),
        enc(client_key),
        enc(name)
    )
}

/// The seat, holding a pairing code (proven inside the envelope, never sent).
pub fn seat_pair(relay: &str, rid: &str, client_key: &str, name: &str) -> String {
    join(relay, rid, client_key, name) + "&pair=1"
}

/// A seatless control room.
pub fn control(relay: &str, rid: &str, client_key: &str, name: &str) -> String {
    join(relay, rid, client_key, name) + "&control=1&strict=1"
}

/// A view room on exactly `instance` (P6): seatless and strict, as `multi.js` dials it.
pub fn view(relay: &str, rid: &str, client_key: &str, name: &str, instance: &str) -> String {
    join(relay, rid, client_key, name) + "&instance=" + &enc(instance) + "&control=1&strict=1"
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn control_url_has_control_and_strict() {
        assert_eq!(
            control("https://relay.example/", "528-491-307", "ck", "Pixel 7"),
            "wss://relay.example/join/528491307?client=ck&name=Pixel%207&control=1&strict=1"
        );
    }

    #[test]
    fn seat_pair_url_matches_relay_link_js() {
        // relay_link.js: base + "/join/" + id + "?client=" + key + "&name=" + name + "&pair=1".
        assert_eq!(
            seat_pair("http://127.0.0.1:4983", "999000777", "k", "n"),
            "ws://127.0.0.1:4983/join/999000777?client=k&name=n&pair=1"
        );
    }

    #[test]
    fn names_are_percent_encoded() {
        let u = control("https://r", "1", "a/b+c", "Android · App & co");
        assert!(u.contains("client=a%2Fb%2Bc"), "{u}");
        assert!(u.contains("name=Android%20%C2%B7%20App%20%26%20co"), "{u}");
        let v = view("https://r", "1", "k", "n", "1:2");
        assert!(v.contains("&instance=1%3A2&control=1&strict=1"), "{v}");
    }
}
