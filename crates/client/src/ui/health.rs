//! The at-a-glance verdict strip: one coloured line naming who is at fault.
//!
//! Every other readout on the stage is a *number*, and a number only helps someone who already
//! knows what it should be. `27 ms rtt, 3.1% loss, 9 ms decode` is three facts and no
//! conclusion; this strip is the conclusion, and the numbers behind it are what it says next.
//!
//! Kept apart from `stage` because the rule is different: the fps/ping readouts are debug
//! views, switchable off by someone who wants a clean picture. This is closer to the software
//! encoding banner — it exists precisely for the moment something is wrong — so it has its own
//! debug item defaulting **on**, and it renders nothing at all while everything is healthy.

use dioxus::prelude::*;

use crate::{debug, state::Ui};

/// Megabits where it reads better, kilobits where it does not. A stream is discussed in Mbps
/// and a still screen in kbps, and one unit for both makes one of the two unreadable.
fn rate(kbps: f64) -> String {
    if kbps >= 1000.0 {
        format!("{:.1} Mbps", kbps / 1000.0)
    } else {
        format!("{kbps:.0} kbps")
    }
}

/// Two rates that are being compared, printed with **one** unit between them.
///
/// `4.8 Mbps of 5.7 Mbps` is the same comparison as `4.8 / 5.7 Mbps` and half again as wide; on a
/// phone the difference decided whether the strip fitted. Falls back to two full rates when the
/// numbers land in different units, because `600 / 5.7 Mbps` would be a lie.
fn pair(got: f64, need: f64) -> String {
    let both_mbps = got >= 1000.0 && need >= 1000.0;
    let both_kbps = got < 1000.0 && need < 1000.0;
    if both_mbps {
        format!("{:.1} / {:.1} Mbps", got / 1000.0, need / 1000.0)
    } else if both_kbps {
        format!("{got:.0} / {need:.0} kbps")
    } else {
        format!("{} / {}", rate(got), rate(need))
    }
}

pub fn render(ui: Ui) -> Element {
    let live = ui.live;
    if !(live.session_on)() || !debug::on(ui, "health") {
        return rsx! {};
    }
    let h = (live.health)();
    if h.state.is_empty() {
        return rsx! {};
    }

    // Healthy is a dot and the bandwidth, nothing else. A strip that reads "everything is
    // fine" in full prose every second is chrome, and this one sits over the picture.
    let ok = h.state == "ok";
    let label = if ok {
        "healthy".to_string()
    } else {
        h.side.clone()
    };

    // Needed vs available, side by side, because that pair is the whole bandwidth question and
    // either number alone answers nothing: 600 kbps arriving is healthy for a still screen and
    // a catastrophe for a moving one, and only the target says which.
    // Split in two so the phone can drop the second half. The pair is the question — is the
    // stream getting what it asked for — and the link figure is supporting evidence.
    let bandwidth = match (h.got_kbps, h.need_kbps) {
        (Some(got), Some(need)) if need > 0.0 => pair(got, need),
        (Some(got), _) => rate(got),
        _ => String::new(),
    };
    let link = h.have_kbps.map(rate).unwrap_or_default();

    rsx! {
        div { class: "health health-{h.state}",
            span { class: "healthdot" }
            span { class: "healthside", "{label}" }
            if !h.detail.is_empty() {
                span { class: "healthdetail", "{h.detail}" }
            }
            if !bandwidth.is_empty() {
                span { class: "healthbw", "{bandwidth}" }
            }
            if !link.is_empty() {
                span { class: "healthlink", title: "measured link capacity", "↓{link}" }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_shared_unit_is_printed_once() {
        // The whole point: half the width of "4.8 Mbps of 5.7 Mbps".
        assert_eq!(pair(4800.0, 5676.0), "4.8 / 5.7 Mbps");
        assert_eq!(pair(600.0, 900.0), "600 / 900 kbps");
    }

    #[test]
    fn a_straddled_boundary_keeps_both_units() {
        // "600 / 5.7 Mbps" would read as 600 Mbps. Width is not worth a wrong number.
        assert_eq!(pair(600.0, 5676.0), "600 kbps / 5.7 Mbps");
        assert_eq!(pair(4800.0, 900.0), "4.8 Mbps / 900 kbps");
    }

    #[test]
    fn the_boundary_itself_is_mbps_on_both_sides() {
        // 1000 kbps is the switch-over in `rate`; `pair` must agree with it or the two readouts
        // disagree about the same number.
        assert_eq!(pair(1000.0, 1000.0), "1.0 / 1.0 Mbps");
        assert_eq!(rate(1000.0), "1.0 Mbps");
        assert_eq!(rate(999.0), "999 kbps");
    }
}
