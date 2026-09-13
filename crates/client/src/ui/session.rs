//! Session group: everything read once when a session starts.
//!
//! Every control here is disabled while a session runs. That is the whole point of the
//! grouping — the compositor sizes its output and configures its encoder and input at Start
//! and cannot be re-sized afterwards (invariant #8), so a control that looks editable but
//! silently does nothing is worse than one that is visibly locked.

use dioxus::prelude::*;

use crate::state::Ui;

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let on = (ui.live.session_on)();
    let custom_res = (s.res)() == "custom";
    // Derived from the viewing device, so a stream can fill it edge to edge instead of
    // being letterboxed into a 16:9 box on a 20:9 panel.
    let device = crate::res::options((ui.live.screen_w)(), (ui.live.screen_h)());
    let custom_q = (s.quality)() == "custom";

    rsx! {
        label { "Resolution" }
        select {
            value: "{(s.res)()}", disabled: on,
            onchange: move |e| s.res.set(e.value()),
            for (value, label) in device.iter().cloned() {
                option { key: "{value}", value: "{value}", "{label}" }
            }
            option { value: "1280x720", "1280 × 720 (720p)" }
            option { value: "1920x1080", "1920 × 1080 (1080p)" }
            option { value: "custom", "Custom…" }
        }
        if custom_res {
            div { class: "row",
                input {
                    r#type: "number", min: "16", step: "2", disabled: on,
                    value: "{(s.custom_w)()}",
                    oninput: move |e| if let Ok(v) = e.value().parse() { s.custom_w.set(v) },
                }
                input {
                    r#type: "number", min: "16", step: "2", disabled: on,
                    value: "{(s.custom_h)()}",
                    oninput: move |e| if let Ok(v) = e.value().parse() { s.custom_h.set(v) },
                }
            }
        }

        label { "Scale" }
        select {
            value: "{(s.scale)()}", disabled: on,
            onchange: move |e| s.scale.set(e.value()),
            for (value, _, label) in crate::res::SCALES {
                option { key: "{value}", value: "{value}", "{label}" }
            }
        }
        p { class: "hint",
            "How large apps draw themselves, as Hyprland's monitor scale does. \
             The stream stays at the resolution above; only the logical area apps lay out \
             in shrinks, so text and buttons come out bigger. Fractional steps need an app \
             that speaks wp-fractional-scale; one that does not falls back to the nearest \
             whole number below and comes out slightly soft."
        }

        label { "FPS" }
        select {
            value: "{(s.fps)()}", disabled: on,
            onchange: move |e| if let Ok(v) = e.value().parse() { s.fps.set(v) },
            // 90 is the useful middle rung, not a compromise: it cuts the pixel rate a
            // fifth below 120 — so every frame gets ~33% more bits at the same bitrate —
            // while keeping the 11.1 ms frame period well under the ~16.7 ms that 60 makes
            // visible as motion judder. Nothing in the pipeline needs a divisor of 60: the
            // render timer derives its period from fps directly.
            //
            // Each rung carries what it costs *on this screen*, because the warning under the
            // picker was not enough. Measured 2026-09-13: a session ran at 120 fps on a 60 Hz
            // phone for hours — `hz=60 ratio=2.00` — with half of every frame rendered,
            // encoded, sent and decoded to be displayed nowhere. The hint saying so was on
            // screen the entire time. Advice below a control is not a control; the consequence
            // belongs in the option the user is actually reading.
            for rung in [30u32, 60, 90, 120] {
                option { key: "{rung}", value: "{rung}", "{fps_label(rung, (ui.live.refresh_hz)())}" }
            }
        }
        // Measured, not guessed — there is no API for the refresh rate, so `js/refresh.js`
        // times requestAnimationFrame gaps. Worth showing because a rung above the panel's
        // rate is not a free upgrade: it splits the same bitrate across more frames the
        // display never shows, which is a quality loss for nothing.
        // The rung labels now carry the per-option consequence, so this says the thing they
        // cannot: *why* an unshown frame is worse than merely wasted. The bitrate is fixed, so
        // frames the screen cannot show still take their share of it.
        if let Some(hz) = (ui.live.refresh_hz)().filter(|h| *h > 0) {
            if (s.fps)() > hz {
                p { class: "hint",
                    "The bitrate is shared across every frame, including the ones this screen \
                     has no refresh to show. Dropping to {hz} does not cost you anything you \
                     can see — it gives the frames you *can* see the bits the others were taking."
                }
            }
        }

        // Not disabled while a session runs, and that is the point: this is what you reach for
        // *when* the rate starts walking. The flag is browser-side, so it takes effect on the
        // next health tick with no reconfigure and no restart. See plan/sync.md §1.
        label {
            class: "check",
            input {
                r#type: "checkbox", checked: (s.fps_lock)(),
                onchange: move |e| {
                    let on = e.checked();
                    s.fps_lock.set(on);
                    crate::bridge::call(format!("window.__wado.setFpsLock({on});"));
                }
            }
            "Lock frame rate (like vsync)"
        }
        p { class: "hint",
            "Normally wado lowers the frame rate when your connection cannot keep up, so what \
             arrives stays smooth at a slower rate. Locked, it never does: you keep the rate \
             above and congestion shows up as stutter instead. Worth it when the rate walking \
             up and down is more distracting than the odd dropped frame — which is the usual \
             case on mobile data at 90 or 120."
        }

        label { "Quality" }
        select {
            value: "{(s.quality)()}", disabled: on,
            onchange: move |e| s.quality.set(e.value()),
            option { value: "reactivity", "Optimize reactivity (low latency)" }
            option { value: "balanced", "Balanced" }
            option { value: "quality", "Optimize image quality" }
            option { value: "custom", "Custom bitrate…" }
        }
        if custom_q {
            label { "Bitrate: {(s.bitrate)()} kbps" }
            input {
                r#type: "range", min: "500", max: "20000", step: "500", disabled: on,
                value: "{(s.bitrate)()}",
                oninput: move |e| if let Ok(v) = e.value().parse() { s.bitrate.set(v) },
            }
        }

        label { "Encoder" }
        select {
            value: "{(s.encoder_backend)()}", disabled: on,
            onchange: move |e| s.encoder_backend.set(e.value()),
            option { value: "auto", "Auto (hardware if available)" }
            option { value: "hardware", "Hardware only (GPU)" }
            option { value: "software", "Software (x264)" }
        }

        label { "Window placement" }
        select {
            value: "{(s.placement)()}", disabled: on,
            onchange: move |e| s.placement.set(e.value()),
            option { value: "center", "Center" }
            option { value: "top_left", "Top-left" }
            option { value: "cascade", "Cascade" }
            option { value: "maximized", "Maximized" }
        }

        label {
            class: "check",
            input {
                r#type: "checkbox", checked: (s.focus_follows)(), disabled: on,
                onchange: move |e| s.focus_follows.set(e.checked()),
            }
            " Focus follows pointer"
        }

        label { "Keyboard repeat" }
        div { class: "row",
            input {
                r#type: "number", min: "1", disabled: on, value: "{(s.repeat_rate)()}",
                oninput: move |e| if let Ok(v) = e.value().parse() { s.repeat_rate.set(v) },
            }
            input {
                r#type: "number", min: "0", disabled: on, value: "{(s.repeat_delay)()}",
                oninput: move |e| if let Ok(v) = e.value().parse() { s.repeat_delay.set(v) },
            }
        }
        p { class: "hint", "rate (keys/s) · delay (ms)" }

        details { class: "sub",
            summary { "Encoder internals" }
            label { "x264 preset" }
            select {
                value: "{(s.preset)()}", disabled: on,
                onchange: move |e| s.preset.set(e.value()),
                option { value: "", "(from quality)" }
                option { value: "ultrafast", "ultrafast" }
                option { value: "superfast", "superfast" }
                option { value: "veryfast", "veryfast" }
                option { value: "faster", "faster" }
            }
            label { "Keyframe interval (frames)" }
            input {
                r#type: "number", min: "1", placeholder: "(from quality)", disabled: on,
                value: "{(s.keyframe)()}",
                oninput: move |e| s.keyframe.set(e.value()),
            }
        }
    }
}

/// What a frame-rate rung costs on the screen it will be shown on.
///
/// Three cases, and the middle one is the one nobody expects:
///
/// * **above the panel's rate** — the excess frames are not "extra smoothness", they are never
///   displayed at all, and they take their share of a fixed bitrate with them. 120 on a 60 Hz
///   panel throws away half of everything the pipeline does.
/// * **at or below, dividing evenly** — every frame is held for a whole number of refreshes, so
///   motion is even. This is what to pick.
/// * **at or below, not dividing** — 60 on a 90 Hz panel alternates 2-refresh and 1-refresh
///   frames. That is 3:2 pulldown, and it is visible as stutter to people who could not tell you
///   why. Every number in the log looks perfect while it happens.
///
/// With no measurement yet (`None`) the rung is named and nothing is claimed.
fn fps_label(rung: u32, hz: Option<u32>) -> String {
    let Some(hz) = hz.filter(|h| *h > 0) else {
        return rung.to_string();
    };
    if rung > hz {
        // Integer percent of frames the panel has no refresh to show.
        let wasted = (rung - hz) * 100 / rung;
        format!("{rung} — {wasted}% never shown on this {hz} Hz screen")
    } else if hz % rung == 0 {
        format!("{rung} — even on this {hz} Hz screen")
    } else {
        format!("{rung} — uneven on this {hz} Hz screen")
    }
}

#[cfg(test)]
mod fps_label_tests {
    use super::fps_label;

    #[test]
    fn unknown_panel_rate_claims_nothing() {
        assert_eq!(fps_label(120, None), "120");
        // A zero reading is a failed measurement, not a 0 Hz screen.
        assert_eq!(fps_label(120, Some(0)), "120");
    }

    #[test]
    fn above_the_panel_says_what_is_lost() {
        // The case measured in the field: half of every frame discarded unseen.
        assert_eq!(
            fps_label(120, Some(60)),
            "120 — 50% never shown on this 60 Hz screen"
        );
        assert_eq!(
            fps_label(90, Some(60)),
            "90 — 33% never shown on this 60 Hz screen"
        );
    }

    #[test]
    fn dividing_evenly_is_the_one_to_pick() {
        assert_eq!(fps_label(60, Some(60)), "60 — even on this 60 Hz screen");
        assert_eq!(fps_label(30, Some(60)), "30 — even on this 60 Hz screen");
        assert_eq!(fps_label(90, Some(90)), "90 — even on this 90 Hz screen");
    }

    #[test]
    fn below_but_not_dividing_is_the_trap() {
        // 3:2 pulldown — alternating 2-refresh and 1-refresh frames.
        assert_eq!(fps_label(60, Some(90)), "60 — uneven on this 90 Hz screen");
    }
}
