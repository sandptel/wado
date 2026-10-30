//! A pairing link, and a typed code. A computer's QR (`wado qr`, `wado tui` → Pair) holds
//! `<client url>?relay=…&id=…&pair=…&hk=…`, or the same as `wado://pair?…`; anything else in the
//! query (the web client's `go=`) is ignored. A typed code is eight characters of Crockford
//! base32, `XXXX-XXXX` (`crates/server/src/gate/typed_code.rs`, `js/pair.js`).

use percent_encoding::percent_decode_str;
use wado_protocol::relay_wire::normalize_remote_id;

/// The alphabet `wado pair` draws typed codes from: no I, L, O or U.
const ALPHABET: &[u8; 32] = b"0123456789ABCDEFGHJKMNPQRSTVWXYZ";

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum PairCode {
    /// The QR's long code.
    Qr(String),
    /// A typed code, normalised.
    Typed([u8; 8]),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PairLink {
    pub relay_url: String,
    /// Digits only.
    pub remote_id: String,
    pub code: PairCode,
    /// The computer's identity pin from the QR, when the link carries one.
    pub hk: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum LinkError {
    NotALink,
    NoRemoteId,
    NoRelay,
    NoCode,
}

impl std::fmt::Display for LinkError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(match self {
            LinkError::NotALink => "that is not a wado pairing link",
            LinkError::NoRemoteId => "the link names no computer",
            LinkError::NoRelay => "the link names no relay",
            LinkError::NoCode => {
                "the link carries no pairing code — show a fresh one on the computer"
            }
        })
    }
}

fn query(link: &str) -> Option<Vec<(String, String)>> {
    let (scheme, rest) = link.trim().split_once(':')?;
    if !matches!(
        scheme.to_ascii_lowercase().as_str(),
        "https" | "http" | "wado"
    ) {
        return None;
    }
    let q = rest.split_once('?')?.1;
    let q = q.split('#').next().unwrap_or("");
    Some(
        q.split('&')
            .filter_map(|kv| {
                let (k, v) = kv.split_once('=')?;
                let dec = |s: &str| {
                    percent_decode_str(&s.replace('+', " "))
                        .decode_utf8_lossy()
                        .into_owned()
                };
                Some((dec(k), dec(v)))
            })
            .collect(),
    )
}

/// Parse a scanned or pasted pairing link.
pub fn parse(link: &str) -> Result<PairLink, LinkError> {
    let q = query(link).ok_or(LinkError::NotALink)?;
    let get = |k: &str| {
        q.iter()
            .find(|(kk, v)| kk == k && !v.trim().is_empty())
            .map(|(_, v)| v.trim().to_string())
    };
    let remote_id: String = get("id")
        .map(|s| normalize_remote_id(&s))
        .unwrap_or_default()
        .chars()
        .filter(char::is_ascii_digit)
        .collect();
    if remote_id.is_empty() {
        return Err(LinkError::NoRemoteId);
    }
    let relay_url = get("relay").ok_or(LinkError::NoRelay)?;
    if !(relay_url.starts_with("https://") || relay_url.starts_with("http://")) {
        return Err(LinkError::NoRelay);
    }
    let code = get("pair").ok_or(LinkError::NoCode)?;
    Ok(PairLink {
        relay_url: relay_url.trim_end_matches('/').to_string(),
        remote_id,
        code: PairCode::Qr(code),
        hk: get("hk"),
    })
}

/// What a person typed, as the computer stores it: separators dropped, upper case, and the
/// look-alikes read as the digits they resemble (`I`/`L` → `1`, `O` → `0`). `None` unless that
/// leaves exactly eight characters of the alphabet.
pub fn typed(input: &str) -> Option<[u8; 8]> {
    let code: Vec<u8> = input
        .chars()
        .filter(|c| !c.is_whitespace() && *c != '-')
        .map(|c| match c.to_ascii_uppercase() {
            'I' | 'L' => '1',
            'O' => '0',
            c => c,
        })
        .map(|c| if c.is_ascii() { c as u8 } else { b'?' })
        .collect();
    if code.len() != 8 || !code.iter().all(|b| ALPHABET.contains(b)) {
        return None;
    }
    let mut out = [0u8; 8];
    out.copy_from_slice(&code);
    Some(out)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_qr_link_from_wado_qr() {
        // `wado qr --relay http://r.example --id 123456789`, as scripts/multi-e2e.mjs mints it.
        let l = parse(
            "https://sandptel.github.io/wado/?relay=http://r.example&id=123456789\
             &pair=Zq8xY2kLmN3pQ4rS5tU6vW7x&hk=abcDEF_-123",
        )
        .unwrap();
        assert_eq!(l.relay_url, "http://r.example");
        assert_eq!(l.remote_id, "123456789");
        assert_eq!(l.code, PairCode::Qr("Zq8xY2kLmN3pQ4rS5tU6vW7x".into()));
        assert_eq!(l.hk.as_deref(), Some("abcDEF_-123"));
        // The web client's `go=` is ignored, and an encoded relay is decoded.
        let l = parse("https://c/?go=files&relay=https%3A%2F%2Fr.example%2F&id=528-491-307&pair=X")
            .unwrap();
        assert_eq!(l.relay_url, "https://r.example");
        assert_eq!(l.remote_id, "528491307");
        assert_eq!(l.hk, None);
    }

    #[test]
    fn parses_wado_scheme() {
        let l = parse("wado://pair?relay=https://r.example&id=1&pair=CODE&hk=H").unwrap();
        assert_eq!(l.remote_id, "1");
        assert_eq!(l.code, PairCode::Qr("CODE".into()));
    }

    #[test]
    fn link_errors_are_specific() {
        assert_eq!(parse("hello"), Err(LinkError::NotALink));
        assert_eq!(parse("ftp://x/?id=1"), Err(LinkError::NotALink));
        assert_eq!(
            parse("https://c/?relay=https://r"),
            Err(LinkError::NoRemoteId)
        );
        assert_eq!(parse("https://c/?id=1"), Err(LinkError::NoRelay));
        assert_eq!(
            parse("https://c/?id=1&relay=javascript:x"),
            Err(LinkError::NoRelay)
        );
        assert_eq!(
            parse("https://c/?id=1&relay=https://r"),
            Err(LinkError::NoCode)
        );
    }

    #[test]
    fn typed_code_normalises_dash_and_case() {
        for t in ["k7qf2mxp", "K7QF-2MXP", "k7qf 2mxp", " K7QF - 2mxp "] {
            assert_eq!(typed(t), Some(*b"K7QF2MXP"), "{t:?}");
        }
        assert_eq!(typed("i0lo-ab12"), Some(*b"1010AB12"));
    }

    #[test]
    fn typed_code_rejects_ambiguous_letters() {
        // U is not in `gate/typed_code.rs`'s alphabet; lengths must be exact.
        assert_eq!(typed("K7QF2MXU"), None);
        assert_eq!(typed("K7QF2MX"), None);
        assert_eq!(typed("K7QF2MXPP"), None);
        assert_eq!(typed("K7QF2MXÉ"), None);
        assert_eq!(typed(""), None);
    }
}
