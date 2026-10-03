//! Display & stream: the session's shape, frame rate, scale and tuning, and what its apps get.
//!
//! Every control here is live-editable. Shape and tuning go to the running session through
//! Apply (a reconfigure keeps the apps); isolation, the X server and the encoder only reach a
//! new session, and say so.

use dioxus::prelude::*;

use super::opts;
use crate::{
    bridge, res,
    state::Ui,
    ui::widgets::{Info, Seg, SwitchRow, When, WhenBadge},
};

const SCALE_INFO: &str = "How large apps draw themselves, as Hyprland's monitor scale does. The \
    stream stays at the resolution above; only the logical area apps lay out in shrinks, so text \
    and buttons come out bigger. Fractional steps need an app that speaks wp-fractional-scale; \
    one that does not falls back to the nearest whole number below and comes out slightly soft.";
const LOCK_INFO: &str = "Normally wado lowers the frame rate when your connection cannot keep up, \
    so what arrives stays smooth at a slower rate. Locked, it never does: you keep the rate and \
    congestion shows up as stutter instead. Worth it when the rate walking up and down is more \
    distracting than the odd dropped frame — the usual case on mobile data at 90 or 120.";
const ISOLATE_INFO: &str = "On, the session gets its own D-Bus bus and its apps are launched with \
    no X11 display. That stops an app you launch here from opening a window on the computer \
    running wado instead — browsers and file managers hand the request to the copy already \
    running over there, and anything X11 draws on its screen. Off, apps share that computer's \
    desktop services: notifications and file-chooser portals work, and X11-only apps open over \
    there rather than failing.";
const X_INFO: &str = "wado speaks Wayland only, so an app that can only speak X11 — Steam is the \
    one people hit — cannot draw here at all. On, the session gets its own X server and those \
    apps run inside it. The cost: every X app shares one screen, a single window the size of the \
    stream that is there even when nothing uses it, with no window manager inside it.";
const ORIENT_INFO: &str = "Which way round the session is. A phone at rest is held upright, so an \
    unthinking start gave desktop applications a 360px-wide screen to lay themselves out on — \
    hence the default. Entering fullscreen pins the phone to whichever way this makes the \
    session, and leaving fullscreen by any route releases it.";

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let live = ui.live;
    let limits = (live.host)().limits;
    let (sw, sh) = ((live.screen_w)(), (live.screen_h)());
    let phone = (live.screen_phone)();
    let res_now = (s.res)();
    let hz = (live.refresh_hz)().filter(|h| *h > 0);

    // The shapes worth a chip: this device's exact sizes, then the catalog's same-way ones.
    let device = res::options(sw, sh, phone);
    let exclude: Vec<String> = device.iter().map(|(v, _)| v.clone()).collect();
    let (same, turned) = res::catalog::grouped(sw, sh, &exclude);
    let chips: Vec<(String, String, bool)> = device
        .iter()
        .map(|(v, _)| (v.clone(), "fits exactly".to_string(), true))
        .chain(
            same.iter()
                .take(3)
                .map(|(v, l)| (v.clone(), short(l), false)),
        )
        .collect();
    let in_chips = chips.iter().any(|(v, _, _)| *v == res_now);
    let mut more = use_signal(|| false);
    let show_more = more() || (!in_chips && !res_now.is_empty());
    let fit = parse_wh(&res_now).map(|(w, h)| res::fit::verdict(sw, sh, w, h));
    let capped = |v: u32| limits.max_fps.is_some_and(|m| v > m);

    rsx! {
        div { class: "card",
            div { class: "cardhead", "Shape" WhenBadge { when: When::Apply } }
            div { class: "shapes",
                for (v, sub, exact) in chips.into_iter() {
                    {
                        let (w, h) = parse_wh(&v).unwrap_or((16, 9));
                        let k = 34.0 / w.max(h) as f64;
                        let style = format!("width:{:.0}px;height:{:.0}px", w as f64 * k, h as f64 * k);
                        let on = v == res_now;
                        let text = v.replace('x', "×");
                        rsx! {
                            button {
                                key: "{v}",
                                class: if on { "shape on" } else { "shape" },
                                onclick: move |_| s.res.set(v.clone()),
                                i { class: if exact { "exact" } else { "" }, style: "{style}" }
                                b { "{text}" }
                                small { "{sub}" }
                            }
                        }
                    }
                }
                button {
                    class: if show_more { "shape on more" } else { "shape more" },
                    onclick: move |_| more.set(!more()),
                    i { class: "dotsbox" }
                    b { "More" }
                    small { "every size" }
                }
            }
            if show_more {
                select {
                    value: "{res_now}",
                    onchange: move |e| s.res.set(e.value()),
                    optgroup { label: "This screen",
                        for (v, l) in device.iter().cloned() { option { key: "{v}", value: "{v}", "{l}" } }
                    }
                    optgroup { label: "Same way round",
                        for (v, l) in same.iter().cloned() { option { key: "{v}", value: "{v}", "{l}" } }
                    }
                    optgroup { label: "Turned",
                        for (v, l) in turned.iter().cloned() { option { key: "{v}", value: "{v}", "{l}" } }
                    }
                    option { value: "custom", "Custom…" }
                }
                if res_now == "custom" {
                    div { class: "row",
                        input {
                            r#type: "number", min: "16", step: "2", "aria-label": "Width",
                            value: "{(s.custom_w)()}",
                            oninput: move |e| if let Ok(v) = e.value().parse() { s.custom_w.set(v) },
                        }
                        input {
                            r#type: "number", min: "16", step: "2", "aria-label": "Height",
                            value: "{(s.custom_h)()}",
                            oninput: move |e| if let Ok(v) = e.value().parse() { s.custom_h.set(v) },
                        }
                    }
                }
            }
            if let Some(note) = fit { p { class: "why", "{note}" } }
            if let (Some(mw), Some(mh)) = (limits.max_width, limits.max_height) {
                p { class: "why host", "This computer caps sessions at {mw}×{mh}." }
            }
        }

        div { class: "card",
            div { class: "cardhead", "Orientation" WhenBadge { when: When::Restart } Info { text: ORIENT_INFO } }
            Seg {
                options: opts(&[("auto", "Auto"), ("landscape", "Landscape"), ("portrait", "Portrait")]),
                value: (s.orientation)(),
                onpick: move |v| { s.orientation.set(v); super::super::live::apply(ui); },
            }
        }

        div { class: "card",
            div { class: "cardhead", "Frame rate" WhenBadge { when: When::Apply } }
            Seg {
                options: [30u32, 60, 90, 120].iter().map(|r| (r.to_string(), if capped(*r) { format!("{r}·") } else { r.to_string() })).collect::<Vec<_>>(),
                value: (s.fps)().to_string(),
                onpick: move |v: String| if let Ok(n) = v.parse::<u32>() { s.fps.set(n) },
            }
            p { class: "why", "{fps_note((s.fps)(), hz)}" }
            if let Some(m) = limits.max_fps {
                if (s.fps)() > m { p { class: "why host", "This computer caps the frame rate at {m}." } }
            }
        }

        div { class: "card",
            div { class: "cardhead", "Scale" WhenBadge { when: When::Apply } Info { text: SCALE_INFO } }
            Seg {
                options: res::SCALES.iter().filter(|(v, _, _)| ["1", "1.25", "1.5", "2", "3"].contains(v)).map(|(v, _, _)| (v.to_string(), format!("{v}×"))).collect::<Vec<_>>(),
                value: (s.scale)(),
                onpick: move |v| s.scale.set(v),
            }
        }

        div { class: "card",
            div { class: "cardhead", "Tuning" WhenBadge { when: When::Apply } }
            Seg {
                options: opts(&[("reactivity", "Reactive"), ("balanced", "Balanced"), ("quality", "Sharp"), ("custom", "Custom")]),
                value: (s.quality)(),
                onpick: move |v| s.quality.set(v),
            }
            if (s.quality)() == "custom" {
                div { class: "slide",
                    input {
                        r#type: "range", min: "500", max: "{limits.max_bitrate.unwrap_or(20000)}", step: "500",
                        "aria-label": "Bitrate",
                        value: "{(s.bitrate)()}",
                        oninput: move |e| if let Ok(v) = e.value().parse() { s.bitrate.set(v) },
                    }
                    span { class: "slidelab", "Bitrate" }
                    span { class: "slideval", "{(s.bitrate)()} kbps" }
                }
            }
            SwitchRow {
                title: "Lock frame rate",
                sub: "Stutter instead of slowing down",
                on: (s.fps_lock)(),
                info: LOCK_INFO.to_string(),
                ontoggle: move |on| {
                    s.fps_lock.set(on);
                    bridge::call(format!("window.__wado.setFpsLock({on});"));
                },
            }
        }

        div { class: "card",
            SwitchRow {
                title: "Low-latency audio",
                sub: "Smaller sound packets and no audio cushion — video waits for sound, so this speeds both",
                on: (s.low_latency_audio)(),
                when: When::Apply,
                ontoggle: move |on| {
                    s.low_latency_audio.set(on);
                    super::super::live::apply(ui);
                    if (ui.live.session_on)() {
                        crate::actions::apply(ui);
                    }
                },
            }
            SwitchRow {
                title: "Auto bitrate",
                sub: match (ui.live.auto_kbps)() {
                    Some(k) if (s.auto_bitrate)() => format!("Capped at {k} kbps — the link can't carry more right now"),
                    _ => "Lowers the bitrate when the link can't carry it, then climbs back".to_string(),
                },
                on: (s.auto_bitrate)(),
                ontoggle: move |on| {
                    s.auto_bitrate.set(on);
                    super::super::live::apply(ui);
                    // Off: the full rate again, now.
                    if !on && (ui.live.auto_kbps)().is_some() && (ui.live.session_on)() {
                        crate::actions::apply(ui);
                    }
                },
            }
        }

        div { class: "card",
            div { class: "cardhead", "Encoder" WhenBadge { when: if limits.encoder.is_some() { When::Host } else { When::Restart } } }
            Seg {
                options: opts(&[("auto", "Auto"), ("hardware", "GPU"), ("software", "x264")]),
                value: (s.encoder_backend)(),
                disabled: limits.encoder.is_some(),
                onpick: move |v| s.encoder_backend.set(v),
            }
        }

        div { class: "card",
            SwitchRow {
                title: "Isolate apps",
                sub: "Own D-Bus, no host X11",
                on: (s.isolate_apps)(),
                when: When::Restart,
                info: ISOLATE_INFO.to_string(),
                ontoggle: move |on| s.isolate_apps.set(on),
            }
            SwitchRow {
                title: "X server",
                sub: "For Steam and X11-only apps",
                on: (s.x_server)(),
                when: When::Restart,
                info: X_INFO.to_string(),
                ontoggle: move |on| s.x_server.set(on),
            }
        }

        details { class: "card sub",
            summary { "Encoder internals" }
            label { "x264 preset" }
            select {
                value: "{(s.preset)()}",
                onchange: move |e| s.preset.set(e.value()),
                option { value: "", "(from tuning)" }
                option { value: "ultrafast", "ultrafast" }
                option { value: "superfast", "superfast" }
                option { value: "veryfast", "veryfast" }
                option { value: "faster", "faster" }
            }
            label { "Keyframe interval (frames)" }
            input {
                r#type: "number", min: "1", placeholder: "(from tuning)",
                value: "{(s.keyframe)()}",
                oninput: move |e| s.keyframe.set(e.value()),
            }
        }
    }
}

/// `"1280x720 — 16:9 landscape · …"` → `"16:9"`: the aspect is what a chip has room for, and the
/// fit verdict under the row says the rest for the selected one.
fn short(label: &str) -> String {
    let rest = label.split_once(" — ").map_or(label, |(_, r)| r);
    let first = rest.split(" · ").next().unwrap_or(rest);
    first.replace(" landscape", "").replace(" portrait", "")
}

/// What the chosen rate costs on this screen. See the cases below.
///
/// * **above the panel's rate** — the excess frames are never displayed, and they take their
///   share of a fixed bitrate with them. 120 on a 60 Hz panel throws away half.
/// * **at or below, dividing evenly** — every frame is held for a whole number of refreshes.
/// * **at or below, not dividing** — 3:2 pulldown, visible as stutter while every number looks
///   perfect.
fn fps_note(rung: u32, hz: Option<u32>) -> String {
    let Some(hz) = hz.filter(|h| *h > 0) else {
        return "This screen's refresh rate is not known yet.".into();
    };
    if rung > hz {
        let wasted = (rung - hz) * 100 / rung;
        format!(
            "{wasted}% of frames are never shown on this {hz} Hz screen — they only cost bitrate."
        )
    } else if hz % rung == 0 {
        format!("Even on this {hz} Hz screen.")
    } else {
        format!("Uneven on this {hz} Hz screen — expect judder. Pick a rate that divides {hz}.")
    }
}

/// `"1920x1080"` → `(1920, 1080)`. `None` for `custom` and anything malformed.
fn parse_wh(v: &str) -> Option<(u32, u32)> {
    let (w, h) = v.split_once('x')?;
    Some((w.trim().parse().ok()?, h.trim().parse().ok()?))
}

#[cfg(test)]
mod tests {
    use super::fps_note;

    #[test]
    fn unknown_panel_rate_claims_nothing() {
        assert!(fps_note(120, None).contains("not known"));
        assert!(fps_note(120, Some(0)).contains("not known"));
    }

    #[test]
    fn the_three_cases() {
        assert!(fps_note(120, Some(60)).starts_with("50%"));
        assert!(fps_note(30, Some(60)).starts_with("Even"));
        assert!(fps_note(60, Some(90)).starts_with("Uneven"));
    }
}
