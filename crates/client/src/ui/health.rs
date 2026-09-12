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
    let bandwidth = match (h.got_kbps, h.need_kbps, h.have_kbps) {
        (Some(got), Some(need), Some(have)) if need > 0.0 => {
            format!("{} of {} · link {}", rate(got), rate(need), rate(have))
        }
        (Some(got), Some(need), None) if need > 0.0 => format!("{} of {}", rate(got), rate(need)),
        (Some(got), _, Some(have)) => format!("{} · link {}", rate(got), rate(have)),
        (Some(got), _, None) => rate(got),
        _ => String::new(),
    };

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
        }
    }
}
