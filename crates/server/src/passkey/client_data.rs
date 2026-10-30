//! `clientDataJSON`: what the browser itself says it was asked to sign — the ceremony, the
//! challenge and the page's origin. The browser fills it in, not the page, which is what makes
//! the origin worth checking.

use base64::Engine as _;
use serde::Deserialize;

#[derive(Debug, Deserialize)]
struct Raw {
    #[serde(rename = "type")]
    ty: String,
    challenge: String,
    origin: String,
    #[serde(default, rename = "crossOrigin")]
    cross_origin: bool,
}

/// The origin, once `raw` checks out as a `ty` ceremony over `challenge`.
pub fn check(raw: &[u8], ty: &str, challenge: &[u8]) -> Result<String, &'static str> {
    let c: Raw =
        serde_json::from_slice(raw).map_err(|_| "the browser's client data is not JSON")?;
    if c.ty != ty {
        return Err("the browser signed a different kind of request");
    }
    let got = base64::engine::general_purpose::URL_SAFE_NO_PAD
        .decode(c.challenge.trim_end_matches('='))
        .map_err(|_| "the challenge is not base64url")?;
    // Not secret — it went to the device in the clear of the envelope — so a plain compare.
    if got != challenge {
        return Err("that answer was for another challenge — try again");
    }
    if c.cross_origin {
        return Err("the passkey was used from inside another site");
    }
    Ok(c.origin)
}

/// The relying-party id for a page at `origin`: its host. `None` for an origin a passkey may
/// not be made on — anything but HTTPS, except a page on this machine itself.
pub fn rp_id(origin: &str) -> Option<String> {
    let (scheme, rest) = origin.split_once("://")?;
    let host = rest.split('/').next()?;
    // `[::1]:8080` keeps its brackets; anything else drops the port.
    let host = if host.starts_with('[') {
        host.split(']').next()?.trim_start_matches('[')
    } else {
        host.split(':').next()?
    };
    let host = host.to_ascii_lowercase();
    if host.is_empty() || rest.contains('@') {
        return None;
    }
    let local = matches!(host.as_str(), "localhost" | "127.0.0.1" | "::1");
    (scheme == "https" || (scheme == "http" && local)).then_some(host)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn origins_a_passkey_may_be_made_on() {
        assert_eq!(
            rp_id("https://sandptel.github.io").as_deref(),
            Some("sandptel.github.io")
        );
        assert_eq!(
            rp_id("https://Wado.example:8443").as_deref(),
            Some("wado.example")
        );
        assert_eq!(rp_id("http://localhost:8080").as_deref(), Some("localhost"));
        assert_eq!(
            rp_id("http://evil.example"),
            None,
            "plain http off this machine"
        );
        assert_eq!(rp_id("https://user@evil.example"), None);
        assert_eq!(rp_id("file:///x"), None);
        assert_eq!(rp_id("garbage"), None);
    }

    #[test]
    fn checks_ceremony_and_challenge() {
        let raw = br#"{"type":"webauthn.get","challenge":"AQID","origin":"https://a.b"}"#;
        assert_eq!(
            check(raw, "webauthn.get", &[1, 2, 3]).unwrap(),
            "https://a.b"
        );
        assert!(check(raw, "webauthn.create", &[1, 2, 3]).is_err());
        assert!(check(raw, "webauthn.get", &[1, 2, 4]).is_err());
        let x = br#"{"type":"webauthn.get","challenge":"AQID","origin":"https://a.b","crossOrigin":true}"#;
        assert!(check(x, "webauthn.get", &[1, 2, 3]).is_err());
        assert!(check(b"nope", "webauthn.get", &[]).is_err());
    }
}
