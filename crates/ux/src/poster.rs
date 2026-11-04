//! The accent slot (base08–0F) an app's poster washes in: its name hashed onto the eight, so a
//! scheme swap recolours the shelf and both clients draw the same app the same colour.
//! ponytail: a hash, not the icon's dominant colour (decoding pixels on the frame is jitter).

pub fn hue(name: &str) -> u8 {
    let h = name.chars().fold(0u32, |a, c| {
        (a.wrapping_mul(31).wrapping_add(c as u32)) & 0xffff
    });
    8 + (h % 8) as u8
}

#[cfg(test)]
mod tests {
    #[test]
    fn stable_and_in_range() {
        for n in ["Celeste", "Steam", "", "ペルソナ"] {
            let h = super::hue(n);
            assert!((8..=15).contains(&h));
            assert_eq!(h, super::hue(n));
        }
    }
}
