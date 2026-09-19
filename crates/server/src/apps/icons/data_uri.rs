//! Wrapping an icon file as a `data:` URI, because the client cannot fetch it any other way.
//!
//! The client is a web page on a phone: it cannot read this machine's filesystem, and in relay
//! mode it has no HTTP route back here at all. So the bytes travel inside the app list — see
//! [`wado_protocol::AppEntry::icon`].

use std::{fs, path::Path};

/// Largest icon file carried inline. The whole list is one JSON message over a phone link, so
/// a single 400 KB PNG is worth more than every other icon in the drawer put together.
const MAX_BYTES: u64 = 32 * 1024;

/// Read a file and wrap it as a `data:` URI, refusing anything too big to carry.
pub fn encode(path: &Path) -> Option<String> {
    let mime = match path.extension()?.to_str()?.to_ascii_lowercase().as_str() {
        "png" => "image/png",
        "svg" => "image/svg+xml",
        _ => return None,
    };
    if fs::metadata(path).ok()?.len() > MAX_BYTES {
        return None;
    }
    let bytes = fs::read(path).ok()?;
    Some(format!("data:{mime};base64,{}", base64(&bytes)))
}

/// Standard base64, padded.
///
/// ponytail: fifteen lines instead of a dependency. This is the only thing in the whole
/// daemon that needs base64, and it needs the encoder only.
fn base64(bytes: &[u8]) -> String {
    const ALPHABET: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::with_capacity(bytes.len().div_ceil(3) * 4);
    for chunk in bytes.chunks(3) {
        let b = [
            chunk[0],
            *chunk.get(1).unwrap_or(&0),
            *chunk.get(2).unwrap_or(&0),
        ];
        let n = u32::from(b[0]) << 16 | u32::from(b[1]) << 8 | u32::from(b[2]);
        for i in 0..4 {
            // A chunk of 1 byte carries 2 meaningful characters, one of 2 carries 3.
            if i <= chunk.len() {
                out.push(ALPHABET[(n >> (18 - i * 6) & 0x3f) as usize] as char);
            } else {
                out.push('=');
            }
        }
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_oversized_file_is_left_out_rather_than_carried() {
        let dir = std::env::temp_dir().join(format!("wado-icons-{}", std::process::id()));
        let _ = fs::create_dir_all(&dir);
        let big = dir.join("big.png");
        fs::write(&big, vec![0u8; MAX_BYTES as usize + 1]).unwrap();
        let small = dir.join("small.png");
        fs::write(&small, b"\x89PNG").unwrap();

        assert_eq!(encode(&big), None, "a huge icon must not ride in the list");
        assert!(encode(&small)
            .unwrap()
            .starts_with("data:image/png;base64,"));
        let _ = fs::remove_dir_all(&dir);
    }
    #[test]
    fn base64_matches_the_rfc_vectors() {
        // The padding cases are the ones that go wrong, so all three lengths are here.
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foob"), "Zm9vYg==");
        assert_eq!(base64(b"fooba"), "Zm9vYmE=");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
        // Bytes above 0x7f exercise the shifting; a signed slip shows up here.
        assert_eq!(base64(&[0xff, 0xfe, 0xfd]), "//79");
        assert_eq!(base64(&[0x00, 0x00, 0x00]), "AAAA");
    }
}
