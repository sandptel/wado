//! What a stream of one shape does to a screen of another.
//!
//! The video is drawn with `object-fit: contain` (`assets/stage.css`), so a mode whose aspect
//! does not match the panel is centred with bars — and **a bar is not part of the session**.
//! Touches there land on nothing, which is the whole of the complaint "I cannot tap
//! everywhere". So every option says, in words, what it will cost before it is chosen.
//!
//! This is arithmetic with an answer that can be checked, which is why it is here and not
//! inlined in the widget that renders it.

/// How a mode sits on a screen.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Fit {
    /// Same aspect: the picture reaches every edge and every pixel of the screen is session.
    Fills,
    /// Narrower than the screen: bars down the left and right.
    Pillarbox,
    /// Wider than the screen: bars along the top and bottom.
    Letterbox,
}

/// Within this much of the screen's aspect, the bars are under a pixel and nobody can see them.
const EXACT: f64 = 0.005;

/// How the mode `(w, h)` sits on a screen of `(sw, sh)`, and the fraction of that screen the
/// picture covers. Coverage is `1.0` when it fills.
pub fn of(sw: u32, sh: u32, w: u32, h: u32) -> (Fit, f64) {
    if sw == 0 || sh == 0 || w == 0 || h == 0 {
        return (Fit::Fills, 1.0);
    }
    let (screen, mode) = (sw as f64 / sh as f64, w as f64 / h as f64);
    // `contain` scales by whichever axis runs out first; the covered area follows from it.
    let k = (sw as f64 / w as f64).min(sh as f64 / h as f64);
    let coverage = (w as f64 * k * (h as f64 * k)) / (sw as f64 * sh as f64);
    if (mode - screen).abs() / screen <= EXACT {
        (Fit::Fills, 1.0)
    } else if mode < screen {
        (Fit::Pillarbox, coverage)
    } else {
        (Fit::Letterbox, coverage)
    }
}

/// The aspect as people write it — "16:9", "20:9" — reduced, with the handful of ratios that
/// have a conventional spelling given theirs.
///
/// A raw gcd is honest and unreadable: a 1080x2400 phone is "9:20", but 1179x2556 reduces to
/// "393:852", which tells nobody anything. So anything that does not reduce small is reported
/// to one decimal place against 9, which is how phone panels are actually described.
pub fn aspect(w: u32, h: u32) -> String {
    if w == 0 || h == 0 {
        return "—".into();
    }
    let (long, short) = if w >= h { (w, h) } else { (h, w) };
    let g = gcd(long, short);
    let (a, b) = (long / g, short / g);
    if b <= 32 {
        format!("{a}:{b}")
    } else {
        format!("{:.1}:9", long as f64 * 9.0 / short as f64)
    }
}

fn gcd(a: u32, b: u32) -> u32 {
    if b == 0 { a.max(1) } else { gcd(b, a % b) }
}

/// Which way round a mode is, in the word a person would use.
pub fn orientation(w: u32, h: u32) -> &'static str {
    if w == h {
        "square"
    } else if w > h {
        "landscape"
    } else {
        "portrait"
    }
}

/// One phrase for an option label: what this mode is, what kind of screen it suits, and what
/// it does *here*. All three, because a number alone is not a suggestion and a suggestion that
/// hides its cost is worse than none.
pub fn label(sw: u32, sh: u32, w: u32, h: u32, class: &str) -> String {
    let (a, o) = (aspect(w, h), orientation(w, h));
    let what = format!("{w} × {h} — {a} {o} · {class}");
    match of(sw, sh, w, h) {
        (Fit::Fills, _) => format!("{what} · fills this screen"),
        (fit, cov) => {
            let lost = ((1.0 - cov) * 100.0).round() as u32;
            let bars = if fit == Fit::Pillarbox {
                "bars at the sides"
            } else {
                "bars top and bottom"
            };
            format!("{what} · {bars}, {lost}% of the screen unused")
        }
    }
}

/// The sentence shown under the picker for whatever is currently chosen. Says what the bars
/// mean rather than only that they exist, because "unused" understates it: those pixels do not
/// belong to the session and a tap on them reaches nothing.
pub fn verdict(sw: u32, sh: u32, w: u32, h: u32) -> String {
    match of(sw, sh, w, h) {
        (Fit::Fills, _) => {
            "Fills this screen edge to edge — every part of it is the session, and a tap \
             anywhere lands in it."
                .into()
        }
        (fit, cov) => {
            let lost = ((1.0 - cov) * 100.0).round() as u32;
            let bars = if fit == Fit::Pillarbox {
                "down each side"
            } else {
                "along the top and bottom"
            };
            format!(
                "This shape is not your screen's: black bars {bars} take {lost}% of it. Those \
                 bars are not part of the session — a tap there reaches nothing. Pick one \
                 marked “fills this screen” to use the whole panel."
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_exact_match_fills() {
        assert_eq!(of(2400, 1080, 1600, 720).0, Fit::Fills);
        assert_eq!(of(2400, 1080, 1600, 720).1, 1.0);
        // Rounding to even pixels moves the aspect a hair; it must still read as filling.
        assert_eq!(of(2556, 1179, 1560, 720).0, Fit::Fills);
    }

    #[test]
    fn a_narrower_mode_gets_side_bars() {
        // 16:9 on a 20:9 phone held landscape.
        let (fit, cov) = of(2400, 1080, 1280, 720);
        assert_eq!(fit, Fit::Pillarbox);
        assert!((cov - 0.8).abs() < 0.01, "{cov}");
    }

    #[test]
    fn a_wider_mode_gets_top_and_bottom_bars() {
        // 21:9 on a 16:9 desktop.
        let (fit, cov) = of(1920, 1080, 2560, 1080);
        assert_eq!(fit, Fit::Letterbox);
        assert!(cov < 1.0 && cov > 0.7, "{cov}");
    }

    #[test]
    fn aspects_are_written_the_way_people_write_them() {
        assert_eq!(aspect(1920, 1080), "16:9");
        assert_eq!(aspect(2400, 1080), "20:9");
        assert_eq!(aspect(1024, 768), "4:3");
        assert_eq!(aspect(2560, 1080), "64:27"); // the honest reduction of 21:9
        // An iPhone 14 panel reduces to 213:71 — useless, so it is described against 9.
        assert_eq!(aspect(2556, 1179), "19.5:9");
    }

    #[test]
    fn degenerate_input_is_not_a_panic() {
        assert_eq!(of(0, 0, 0, 0), (Fit::Fills, 1.0));
        assert_eq!(aspect(0, 5), "—");
    }
}
