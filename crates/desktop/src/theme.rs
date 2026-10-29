// SPDX-License-Identifier: AGPL-3.0-only
//! base16 → the `Theme` global. The schemes are the web client's own list (`schemes.json`), so
//! both apps offer the same sixteen-slot palettes; the derived roles are mixed in OKLab exactly
//! as `base.css` mixes them, so a surface on the desktop is the surface on the phone.

use slint::{Color, ComponentHandle};

use crate::{AppWindow, Theme};

const SCHEMES: &str = include_str!("../../client/src/schemes.json");

/// The default, as on the web.
pub const DEFAULT: &str = "default-dark";

#[derive(Clone)]
pub struct Scheme {
    pub name: String,
    pub slots: [Color; 16],
}

pub fn all() -> Vec<Scheme> {
    let raw: Vec<(String, Vec<String>)> = serde_json::from_str(SCHEMES).unwrap_or_default();
    raw.into_iter()
        .filter_map(|(name, hex)| {
            let v: Vec<Color> = hex.iter().filter_map(|h| parse_hex(h)).collect();
            let slots: [Color; 16] = v.try_into().ok()?;
            Some(Scheme { name, slots })
        })
        .collect()
}

pub fn find(name: &str) -> Scheme {
    let all = all();
    all.iter()
        .find(|s| s.name == name)
        .or_else(|| all.iter().find(|s| s.name == DEFAULT))
        .cloned()
        .unwrap_or(Scheme {
            name: DEFAULT.into(),
            slots: [Color::from_rgb_u8(0x18, 0x18, 0x18); 16],
        })
}

fn parse_hex(h: &str) -> Option<Color> {
    let n = u32::from_str_radix(h.trim_start_matches('#'), 16).ok()?;
    Some(Color::from_rgb_u8((n >> 16) as u8, (n >> 8) as u8, n as u8))
}

/// Write a scheme into the `Theme` global: sixteen slots and the roles mixed from them.
pub fn apply(ui: &AppWindow, s: &Scheme) {
    let t = ui.global::<Theme>();
    let b = &s.slots;
    t.set_base00(b[0]);
    t.set_base01(b[1]);
    t.set_base02(b[2]);
    t.set_base03(b[3]);
    t.set_base04(b[4]);
    t.set_base05(b[5]);
    t.set_base06(b[6]);
    t.set_base07(b[7]);
    t.set_base08(b[8]);
    t.set_base09(b[9]);
    t.set_base0A(b[10]);
    t.set_base0B(b[11]);
    t.set_base0C(b[12]);
    t.set_base0D(b[13]);
    t.set_base0E(b[14]);
    t.set_base0F(b[15]);

    let (g, fg) = (b[0], b[5]);
    t.set_ground(g);
    // The Mixed Ladder Rule: base00 toward base05 at 6 / 11 / 18 %, line at 14 %.
    t.set_surface_1(mix(g, fg, 0.06));
    t.set_surface_2(mix(g, fg, 0.11));
    t.set_surface_3(mix(g, fg, 0.18));
    t.set_line(mix(g, fg, 0.14));
    t.set_bright(fg);
    t.set_text(mix(fg, g, 0.10));
    t.set_dim(mix(fg, g, 0.42));
    t.set_accent(b[13]);
    t.set_on_accent(g);
    t.set_good(b[11]);
    t.set_warn(b[9]);
    t.set_caution(b[10]);
    t.set_bad(b[8]);
    t.set_mauve(b[14]);
    let tint = |c: Color, a: f32| c.with_alpha(a);
    t.set_accent_tint(tint(b[13], 0.18));
    t.set_good_tint(tint(b[11], 0.15));
    t.set_warn_tint(tint(b[9], 0.15));
    t.set_caution_tint(tint(b[10], 0.15));
    t.set_bad_tint(tint(b[8], 0.18));
    t.set_mauve_tint(tint(b[14], 0.16));
    let light = luminance(g) > luminance(fg);
    t.set_light_ground(light);
    let (dark, lite) = if light { (fg, g) } else { (g, fg) };
    t.set_qr_dark(dark);
    t.set_qr_light(lite);
}

/// A computer's or a device's disc colour: the same six slots, in the same order, as the web
/// client's `profile::hue`.
pub fn hue(ui: &AppWindow, i: usize) -> Color {
    let t = ui.global::<Theme>();
    [
        t.get_base0D(),
        t.get_base0E(),
        t.get_base0C(),
        t.get_base0B(),
        t.get_base0A(),
        t.get_base09(),
    ][i % 6]
}

/// A stable index from a string, for a hue that follows a device through restarts.
pub fn hue_index(key: &str) -> usize {
    key.bytes()
        .fold(0usize, |h, b| h.wrapping_mul(31).wrapping_add(b as usize))
}

fn luminance(c: Color) -> f32 {
    let [r, g, b] = [c.red(), c.green(), c.blue()].map(|v| srgb_to_linear(v as f32 / 255.0));
    0.2126 * r + 0.7152 * g + 0.0722 * b
}

/// `color-mix(in oklab, a, b k)`: `a` with `k` of `b`.
pub fn mix(a: Color, b: Color, k: f32) -> Color {
    let la = to_oklab(a);
    let lb = to_oklab(b);
    let m = [0, 1, 2].map(|i| la[i] + (lb[i] - la[i]) * k);
    from_oklab(m)
}

fn srgb_to_linear(c: f32) -> f32 {
    if c <= 0.04045 {
        c / 12.92
    } else {
        ((c + 0.055) / 1.055).powf(2.4)
    }
}

fn linear_to_srgb(c: f32) -> f32 {
    if c <= 0.0031308 {
        c * 12.92
    } else {
        1.055 * c.powf(1.0 / 2.4) - 0.055
    }
}

fn to_oklab(c: Color) -> [f32; 3] {
    let [r, g, b] = [c.red(), c.green(), c.blue()].map(|v| srgb_to_linear(v as f32 / 255.0));
    let l = (0.4122214708 * r + 0.5363325363 * g + 0.0514459929 * b).cbrt();
    let m = (0.2119034982 * r + 0.6806995451 * g + 0.1073969566 * b).cbrt();
    let s = (0.0883024619 * r + 0.2817188376 * g + 0.6299787005 * b).cbrt();
    [
        0.2104542553 * l + 0.7936177850 * m - 0.0040720468 * s,
        1.9779984951 * l - 2.4285922050 * m + 0.4505937099 * s,
        0.0259040371 * l + 0.7827717662 * m - 0.8086757660 * s,
    ]
}

fn from_oklab([l, a, b]: [f32; 3]) -> Color {
    let l_ = (l + 0.3963377774 * a + 0.2158037573 * b).powi(3);
    let m_ = (l - 0.1055613458 * a - 0.0638541728 * b).powi(3);
    let s_ = (l - 0.0894841775 * a - 1.2914855480 * b).powi(3);
    let r = 4.0767416621 * l_ - 3.3077115913 * m_ + 0.2309699292 * s_;
    let g = -1.2684380046 * l_ + 2.6097574011 * m_ - 0.3413193965 * s_;
    let bl = -0.0041960863 * l_ - 0.7034186147 * m_ + 1.7076147010 * s_;
    let to8 = |v: f32| (linear_to_srgb(v.clamp(0.0, 1.0)) * 255.0).round() as u8;
    Color::from_rgb_u8(to8(r), to8(g), to8(bl))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn every_web_scheme_loads() {
        let all = all();
        assert!(all.len() >= 10, "schemes.json parsed: {}", all.len());
        assert!(all.iter().any(|s| s.name == DEFAULT));
    }

    #[test]
    fn a_mix_lands_between_its_ends() {
        let a = Color::from_rgb_u8(0x18, 0x18, 0x18);
        let b = Color::from_rgb_u8(0xd8, 0xd8, 0xd8);
        assert_eq!(mix(a, b, 0.0), a);
        assert_eq!(mix(a, b, 1.0), b);
        let m = mix(a, b, 0.11);
        assert!(m.red() > a.red() && m.red() < b.red());
    }
}
