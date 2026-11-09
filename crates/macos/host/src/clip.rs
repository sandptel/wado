//! This Mac's clipboard (NSPasteboard): text and images both ways, and a change counter the
//! hub follows. Concealed items — password managers mark theirs — are read as secret, so they
//! never reach a phone's history; wado's own writes are not announced back as changes.
//!
//! The pasteboard is polled (`changeCount`, four times a second) only while someone listens,
//! as macOS has no change notification for it.

use std::sync::atomic::{AtomicIsize, Ordering};
use std::sync::{Arc, OnceLock};
use std::time::Duration;

use objc2::rc::Retained;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSPasteboard};
use objc2_foundation::{NSData, NSDictionary, NSString};
use tokio::sync::watch;
use wado_platform::clip::{Clip, Clipboard, is_secret};
use wado_platform::{Fut, Res};

const TEXT: &str = "public.utf8-plain-text";
const PNG: &str = "public.png";
const TIFF: &str = "public.tiff";
/// What macOS clipboard managers read as "do not keep" (nspasteboard.org).
const CONCEALED: &str = "org.nspasteboard.ConcealedType";
/// How often the pasteboard is looked at while someone listens.
const POLL: Duration = Duration::from_millis(250);

pub struct MacClipboard {
    changes: watch::Sender<u64>,
    watching: OnceLock<()>,
    /// The `changeCount` wado's own last write left, which is not a change to announce.
    own: Arc<AtomicIsize>,
}

impl MacClipboard {
    pub fn new() -> MacClipboard {
        MacClipboard {
            changes: watch::channel(0).0,
            watching: OnceLock::new(),
            own: Arc::new(AtomicIsize::new(-1)),
        }
    }
}

impl Default for MacClipboard {
    fn default() -> Self {
        Self::new()
    }
}

fn general() -> Retained<NSPasteboard> {
    NSPasteboard::generalPasteboard()
}

fn ty(s: &str) -> Retained<NSString> {
    NSString::from_str(s)
}

/// What `pb` holds, as wado's clip: an image if there is one, else text.
pub fn read(pb: &NSPasteboard) -> Res<Clip> {
    let types: Vec<String> = pb
        .types()
        .map(|a| a.iter().map(|t| t.to_string()).collect())
        .unwrap_or_default();
    let secret = is_secret(&types);
    let has = |t: &str| types.iter().any(|x| x == t);
    if has(PNG) || has(TIFF) {
        let png = if has(PNG) {
            pb.dataForType(&ty(PNG)).map(|d| d.to_vec())
        } else {
            pb.dataForType(&ty(TIFF)).and_then(|d| to_png(&d))
        };
        if let Some(png) = png {
            return Ok(Clip {
                mime: "image/png".into(),
                data: png.into(),
                secret,
            });
        }
    }
    match pb.stringForType(&ty(TEXT)) {
        Some(s) => Ok(Clip {
            mime: "text/plain;charset=utf-8".into(),
            data: s.to_string().into_bytes().into(),
            secret,
        }),
        None => Err("this Mac's clipboard holds nothing wado can carry (text or an image)".into()),
    }
}

/// Make `clip` what `pb` holds. Returns the pasteboard's `changeCount` after the write.
pub fn write(pb: &NSPasteboard, clip: &Clip) -> Res<isize> {
    pb.clearContents();
    let ok = if clip.is_text() {
        let s = String::from_utf8_lossy(&clip.data);
        pb.setString_forType(&NSString::from_str(&s), &ty(TEXT))
    } else if clip.mime == "image/png" {
        pb.setData_forType(Some(&NSData::with_bytes(&clip.data)), &ty(PNG))
    } else {
        return Err(format!(
            "this Mac's clipboard takes text or PNG images, not {}",
            clip.mime
        ));
    };
    if !ok {
        return Err("macOS refused the clipboard write".into());
    }
    if clip.secret {
        pb.setData_forType(Some(&NSData::new()), &ty(CONCEALED));
    }
    Ok(pb.changeCount())
}

fn to_png(tiff: &NSData) -> Option<Vec<u8>> {
    let rep = NSBitmapImageRep::imageRepWithData(tiff)?;
    // SAFETY: an empty properties dictionary is a valid argument.
    let png = unsafe {
        rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }?;
    Some(png.to_vec())
}

impl Clipboard for MacClipboard {
    fn get(&self) -> Fut<'_, Res<Clip>> {
        Box::pin(async move { read(&general()) })
    }

    fn set(&self, clip: Clip) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            let n = write(&general(), &clip)?;
            self.own.store(n, Ordering::Relaxed);
            Ok(())
        })
    }

    fn changes(&self) -> watch::Receiver<u64> {
        self.watching.get_or_init(|| {
            let (tx, own) = (self.changes.clone(), Arc::clone(&self.own));
            tokio::spawn(follow(tx, own));
        });
        self.changes.subscribe()
    }
}

/// Tick `tx` on every change wado did not make, polling only while someone is subscribed.
async fn follow(tx: watch::Sender<u64>, own: Arc<AtomicIsize>) {
    let mut seen = general().changeCount();
    loop {
        if tx.receiver_count() == 0 {
            tokio::time::sleep(Duration::from_secs(1)).await;
            seen = general().changeCount();
            continue;
        }
        tokio::time::sleep(POLL).await;
        let n = general().changeCount();
        if n != seen {
            seen = n;
            if n != own.load(Ordering::Relaxed) {
                tx.send_modify(|c| *c = c.wrapping_add(1));
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A private pasteboard, so the user's own clipboard is never touched.
    fn private() -> Retained<NSPasteboard> {
        NSPasteboard::pasteboardWithUniqueName()
    }

    #[test]
    fn text_round_trips() {
        let pb = private();
        let clip = Clip {
            mime: "text/plain;charset=utf-8".into(),
            data: "héllo ✓".as_bytes().into(),
            secret: false,
        };
        write(&pb, &clip).unwrap();
        let back = read(&pb).unwrap();
        assert_eq!(&*back.data, "héllo ✓".as_bytes());
        assert!(back.is_text() && !back.secret);
    }

    /// A 1×1 PNG.
    const DOT: &[u8] = &[
        0x89, 0x50, 0x4e, 0x47, 0x0d, 0x0a, 0x1a, 0x0a, 0, 0, 0, 0x0d, 0x49, 0x48, 0x44, 0x52, 0,
        0, 0, 1, 0, 0, 0, 1, 8, 6, 0, 0, 0, 0x1f, 0x15, 0xc4, 0x89, 0, 0, 0, 0x0d, 0x49, 0x44,
        0x41, 0x54, 0x78, 0x9c, 0x63, 0xf8, 0xcf, 0xc0, 0xf0, 0x1f, 0, 0x05, 0, 0x01, 0xff, 0x89,
        0x99, 0x3d, 0x1d, 0, 0, 0, 0, 0x49, 0x45, 0x4e, 0x44, 0xae, 0x42, 0x60, 0x82,
    ];

    #[test]
    fn png_round_trips() {
        let pb = private();
        write(
            &pb,
            &Clip {
                mime: "image/png".into(),
                data: DOT.into(),
                secret: false,
            },
        )
        .unwrap();
        let back = read(&pb).unwrap();
        assert_eq!(back.mime, "image/png");
        assert_eq!(&back.data[..8], &DOT[..8]);
    }

    #[test]
    fn concealed_is_secret() {
        let pb = private();
        write(
            &pb,
            &Clip {
                mime: "text/plain".into(),
                data: b"hunter2".as_slice().into(),
                secret: true,
            },
        )
        .unwrap();
        assert!(read(&pb).unwrap().secret);
    }

    #[test]
    fn own_write_is_not_a_change() {
        let pb = private();
        let n = write(
            &pb,
            &Clip {
                mime: "text/plain".into(),
                data: b"x".as_slice().into(),
                secret: false,
            },
        )
        .unwrap();
        assert_eq!(
            n,
            pb.changeCount(),
            "the count wado records is the one a poll then sees"
        );
    }
}
