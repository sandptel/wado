//! Cover art made small enough to ride in the host state: any picture macOS can read (players
//! hand over TIFF, often hundreds of KiB), scaled to fit [`SIDE`] and sent as a JPEG `data:` URI.

use base64::Engine;
use base64::engine::general_purpose::STANDARD;
use objc2::AllocAnyThread;
use objc2::runtime::AnyObject;
use objc2_app_kit::{NSBitmapImageFileType, NSBitmapImageRep, NSImageCompressionFactor};
use objc2_core_foundation::{CGPoint, CGRect, CGSize};
use objc2_core_graphics::{
    CGBitmapContextCreate, CGBitmapContextCreateImage, CGColorSpace, CGContext, CGImageAlphaInfo,
};
use objc2_foundation::{NSData, NSDictionary, NSNumber};

/// The longest side sent: a phone card shows it at well under this.
const SIDE: f64 = 320.0;

/// `b64` (a picture, base64) as a small JPEG `data:` URI; `None` when it is not a picture.
pub fn data_uri(b64: &str) -> Option<String> {
    let bytes = STANDARD.decode(b64).ok()?;
    let rep = NSBitmapImageRep::imageRepWithData(&NSData::with_bytes(&bytes))?;
    let (w, h) = (rep.pixelsWide() as f64, rep.pixelsHigh() as f64);
    let k = SIDE / w.max(h);
    let rep = if k < 1.0 {
        scaled(&rep, (w * k).round() as usize, (h * k).round() as usize)?
    } else {
        rep
    };
    let q = NSNumber::new_f64(0.75);
    let q: &AnyObject = &q;
    // SAFETY: the key is AppKit's own constant.
    let props = NSDictionary::from_slices(&[unsafe { NSImageCompressionFactor }], &[q]);
    // SAFETY: the properties are a valid dictionary for JPEG.
    let jpeg =
        unsafe { rep.representationUsingType_properties(NSBitmapImageFileType::JPEG, &props) }?;
    Some(format!(
        "data:image/jpeg;base64,{}",
        STANDARD.encode(jpeg.to_vec())
    ))
}

/// An app icon (`tiff`: every size macOS holds of it) as a `side`-pixel PNG `data:` URI, its
/// transparency kept: drawn from the smallest size at least that big.
pub fn png_uri(tiff: &[u8], side: usize) -> Option<String> {
    let reps = NSBitmapImageRep::imageRepsWithData(&NSData::with_bytes(tiff));
    let best = reps
        .iter()
        .filter_map(|r| r.downcast::<NSBitmapImageRep>().ok())
        .min_by_key(|r| {
            let w = r.pixelsWide() as usize;
            if w >= side {
                w - side
            } else {
                usize::MAX / 2 + (side - w)
            }
        })?;
    png_of(&best, side)
}

/// `rep` as a `side`-pixel PNG `data:` URI, transparency kept.
pub fn png_of(rep: &NSBitmapImageRep, side: usize) -> Option<String> {
    let rep = draw(rep, side, side, true)?;
    // SAFETY: empty properties are valid for PNG.
    let png = unsafe {
        rep.representationUsingType_properties(NSBitmapImageFileType::PNG, &NSDictionary::new())
    }?;
    Some(format!(
        "data:image/png;base64,{}",
        STANDARD.encode(png.to_vec())
    ))
}

/// `rep` drawn at `w`×`h`.
fn scaled(
    rep: &NSBitmapImageRep,
    w: usize,
    h: usize,
) -> Option<objc2::rc::Retained<NSBitmapImageRep>> {
    draw(rep, w, h, false)
}

/// `rep` drawn at `w`×`h`; `alpha`: keep transparency (a PNG), else opaque (a JPEG).
fn draw(
    rep: &NSBitmapImageRep,
    w: usize,
    h: usize,
    alpha: bool,
) -> Option<objc2::rc::Retained<NSBitmapImageRep>> {
    let img = rep.CGImage()?;
    let space = CGColorSpace::new_device_rgb()?;
    // SAFETY: a null buffer asks Core Graphics to allocate one of the right size.
    let ctx = unsafe {
        CGBitmapContextCreate(
            std::ptr::null_mut(),
            w,
            h,
            8,
            0,
            Some(&space),
            if alpha {
                CGImageAlphaInfo::PremultipliedLast.0
            } else {
                CGImageAlphaInfo::NoneSkipLast.0
            },
        )
    }?;
    let rect = CGRect::new(CGPoint::new(0.0, 0.0), CGSize::new(w as f64, h as f64));
    CGContext::draw_image(Some(&ctx), rect, Some(&img));
    let out = CGBitmapContextCreateImage(Some(&ctx))?;
    Some(NSBitmapImageRep::initWithCGImage(
        NSBitmapImageRep::alloc(),
        &out,
    ))
}

#[cfg(test)]
mod tests {
    /// A 1×1 PNG.
    const DOT: &str = "iVBORw0KGgoAAAANSUhEUgAAAAEAAAABCAYAAAAfFcSJAAAADUlEQVR42mNk+M9QDwADhgGAWjR9awAAAABJRU5ErkJggg==";

    #[test]
    fn an_app_icon_becomes_a_png_uri() {
        let img = objc2_app_kit::NSWorkspace::sharedWorkspace().iconForFile(
            &objc2_foundation::NSString::from_str("/System/Applications/Calculator.app"),
        );
        let uri = super::png_uri(&img.TIFFRepresentation().unwrap().to_vec(), 96).unwrap();
        assert!(
            uri.starts_with("data:image/png;base64,iVBOR"),
            "{}",
            &uri[..40]
        );
        assert!(uri.len() < 40_000, "small: {}", uri.len());
    }

    #[test]
    fn a_picture_becomes_a_jpeg_uri() {
        let uri = super::data_uri(DOT).unwrap();
        assert!(uri.starts_with("data:image/jpeg;base64,/9j/"), "{uri}");
    }

    #[test]
    fn not_a_picture_is_none() {
        assert_eq!(super::data_uri("aGVsbG8="), None);
    }
}
