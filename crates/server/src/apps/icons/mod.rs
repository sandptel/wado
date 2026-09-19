//! Turning a desktop entry's `Icon=` into something a browser can draw.
//!
//! Two steps, one per submodule: find the best file for the name ([`index`]), then carry it
//! inline as a `data:` URI ([`data_uri`]). They are split because they fail for unrelated
//! reasons — nothing installed under that name, versus a file too large to send — and because
//! the first is a cached filesystem walk while the second is a pure transform.

mod data_uri;
mod index;

use std::path::Path;

/// Resolve `Icon=` to a `data:` URI, or `None` when nothing usable was found.
///
/// An absolute path is taken at its word (the spec allows one); anything else is a name.
/// `None` is normal rather than an error: plenty of entries name an icon no installed theme
/// provides, and the client draws a letter tile for those.
pub fn resolve(icon: &str) -> Option<String> {
    let icon = icon.trim();
    if icon.is_empty() {
        return None;
    }
    if icon.starts_with('/') {
        return data_uri::encode(Path::new(icon));
    }
    data_uri::encode(index::lookup(strip_image_extension(icon))?.as_path())
}

/// `"firefox.png"` → `"firefox"`, but `"org.gnome.Calculator"` → itself.
///
/// A name with an extension still appears in the wild, so it has to be handled — but not with
/// `file_stem`, which is what this replaced. Icon names are reverse-DNS more often than not
/// these days, and `file_stem("org.gnome.Calculator")` is `"org.gnome"`: every GNOME
/// application in the drawer came back without an icon.
fn strip_image_extension(icon: &str) -> &str {
    match icon.rsplit_once('.') {
        Some((stem, "png" | "svg" | "xpm" | "jpg" | "jpeg")) => stem,
        _ => icon,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reverse_dns_icon_name_is_not_mistaken_for_a_file_extension() {
        assert_eq!(strip_image_extension("firefox"), "firefox");
        assert_eq!(strip_image_extension("firefox.png"), "firefox");
        assert_eq!(strip_image_extension("firefox.svg"), "firefox");
        // The regression this exists for.
        assert_eq!(
            strip_image_extension("org.gnome.Calculator"),
            "org.gnome.Calculator"
        );
        assert_eq!(
            strip_image_extension("org.gnome.Calculator.png"),
            "org.gnome.Calculator"
        );
    }
}
