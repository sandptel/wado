//! Clipboard bytes as the wire carries them: text as itself, anything else as a `data:` URI.

use base64::Engine;

use wado_compositor::clipboard::Clip;

pub fn to_wire(clip: &Clip) -> String {
    if clip.is_text() {
        String::from_utf8_lossy(&clip.data).into_owned()
    } else {
        format!(
            "data:{};base64,{}",
            clip.mime,
            base64::engine::general_purpose::STANDARD.encode(&clip.data)
        )
    }
}

/// `data` as sent by a viewer for `mime`. Text mimes become `text/plain;charset=utf-8`; anything
/// else must be an image sent as a `data:` URI.
pub fn from_wire(mime: &str, data: &str) -> Result<Clip, String> {
    if mime.starts_with("text/") {
        return Ok(Clip {
            mime: "text/plain;charset=utf-8".into(),
            data: data.as_bytes().into(),
        });
    }
    if !mime.starts_with("image/") || mime.contains([';', ' ', '\n']) {
        return Err(format!("{mime} cannot go on the clipboard"));
    }
    let b64 = data
        .split_once(";base64,")
        .map(|(_, b)| b)
        .ok_or("an image must be sent as a data: URI")?;
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(b64)
        .map_err(|e| format!("bad image data: {e}"))?;
    Ok(Clip {
        mime: mime.into(),
        data: bytes.into(),
    })
}

/// The image type of `bytes`, from its first bytes.
pub fn sniff(bytes: &[u8]) -> Option<&'static str> {
    Some(match bytes {
        [0x89, b'P', b'N', b'G', ..] => "image/png",
        [0xFF, 0xD8, ..] => "image/jpeg",
        [b'G', b'I', b'F', ..] => "image/gif",
        [
            b'R',
            b'I',
            b'F',
            b'F',
            _,
            _,
            _,
            _,
            b'W',
            b'E',
            b'B',
            b'P',
            ..,
        ] => "image/webp",
        [b'B', b'M', ..] => "image/bmp",
        _ => return None,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips() {
        let img = Clip {
            mime: "image/png".into(),
            data: vec![0x89, b'P', b'N', b'G', 1, 2].into(),
        };
        assert_eq!(from_wire("image/png", &to_wire(&img)).unwrap(), img);
        let t = from_wire("text/plain", "héllo").unwrap();
        assert!(t.is_text());
        assert_eq!(to_wire(&t), "héllo");
        assert!(from_wire("application/x-sh", "x").is_err());
        assert_eq!(sniff(&img.data), Some("image/png"));
    }
}
