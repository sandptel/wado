//! Line icons, drawn as one stroked path each so `currentColor` themes them.
//!
//! Inline SVG rather than an icon font: no request, no flash of missing glyphs, and each icon
//! is a string the compiler can see is unused.

use dioxus::prelude::*;

fn path(name: &str) -> &'static str {
    match name {
        "dots" => "M5 12h.01M12 12h.01M19 12h.01",
        "back" => "M16 5v14L6 12z",
        "kbd" => "M4 6h16a2 2 0 0 1 2 2v8a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V8a2 2 0 0 1 2-2zM6 10h.01M10 10h.01M14 10h.01M18 10h.01M7 14h10",
        "pad" => "M6 11h4M8 9v4M15 12h.01M18 10h.01M17.3 5H6.7a4 4 0 0 0-3.96 3.43L2 14.6A2.9 2.9 0 0 0 7 17l1.5-2h7l1.5 2a2.9 2.9 0 0 0 5-2.4l-.74-6.17A4 4 0 0 0 17.3 5z",
        "target" => "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18zM12 7a5 5 0 1 0 0 10a5 5 0 1 0 0-10zM12 11.5v1",
        "max" => "M8 3H5a2 2 0 0 0-2 2v3M21 8V5a2 2 0 0 0-2-2h-3M3 16v3a2 2 0 0 0 2 2h3M16 21h3a2 2 0 0 0 2-2v-3",
        "move" => "M5 9l-3 3 3 3M9 5l3-3 3 3M15 19l-3 3-3-3M19 9l3 3-3 3M2 12h20M12 2v20",
        "scroll" => "M7 15l5 5 5-5M7 9l5-5 5 5",
        "lock" => "M6 11h12a2 2 0 0 1 2 2v6a2 2 0 0 1-2 2H6a2 2 0 0 1-2-2v-6a2 2 0 0 1 2-2zM8 11V7a4 4 0 0 1 8 0v4",
        "hand" => "M18 11V6a2 2 0 0 0-4 0M14 10V4a2 2 0 0 0-4 0v6M10 10.5V6a2 2 0 0 0-4 0v8a8 8 0 0 0 16 0v-2a2 2 0 0 0-4 0",
        "refresh" => "M21 12a9 9 0 1 1-3-6.7L21 8M21 3v5h-5",
        "monitor" => "M4 3h16a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM8 21h8M12 17v4",
        "layers" => "M12 2 2 7l10 5 10-5-10-5zM2 17l10 5 10-5M2 12l10 5 10-5",
        "palette" => "M12 22a10 10 0 1 1 10-10c0 2.8-2.2 4-4 4h-2a2 2 0 0 0-1.5 3.3A1.6 1.6 0 0 1 12 22zM7.5 10.5h.01M12 7h.01M16.5 10.5h.01",
        "net" => "M5 12.5a10 10 0 0 1 14 0M8.5 16a5 5 0 0 1 7 0M2 9a15 15 0 0 1 20 0M12 20h.01",
        "pulse" => "M22 12h-4l-3 9L9 3l-3 9H2",
        "term" => "M4 17l6-6-6-6M12 19h8",
        "right" => "M9 18l6-6-6-6",
        "left" => "M15 18l-6-6 6-6",
        "down" => "M6 9l6 6 6-6",
        "up" => "M18 15l-6-6-6 6",
        "x" => "M18 6 6 18M6 6l12 12",
        "plus" => "M12 5v14M5 12h14",
        "power" => "M12 2v10M18.4 6.6a9 9 0 1 1-12.8 0",
        "server" => "M4 3h16a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2V5a2 2 0 0 1 2-2zM4 13h16a2 2 0 0 1 2 2v4a2 2 0 0 1-2 2H4a2 2 0 0 1-2-2v-4a2 2 0 0 1 2-2zM6 7h.01M6 17h.01",
        "eye" => "M2 12s3.5-7 10-7 10 7 10 7-3.5 7-10 7S2 12 2 12zM12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6z",
        "sun" => "M12 4V2M12 22v-2M4 12H2M22 12h-2M5.6 5.6 4.2 4.2M19.8 19.8l-1.4-1.4M5.6 18.4l-1.4 1.4M19.8 4.2l-1.4 1.4M12 8a4 4 0 1 0 0 8a4 4 0 1 0 0-8z",
        "search" => "M11 4a7 7 0 1 0 0 14a7 7 0 1 0 0-14zM21 21l-5-5",
        "sliders" => "M4 21v-7M4 10V3M12 21v-9M12 8V3M20 21v-5M20 12V3M1 14h6M9 8h6M17 16h6",
        "edit" => "M12 20h9M16.5 3.5a2.1 2.1 0 0 1 3 3L7 19l-4 1 1-4z",
        "check" => "M20 6 9 17l-5-5",
        "info" => "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18zM12 16v-4M12 8h.01",
        "alert" => "M12 9v4M12 17h.01M10.3 3.9 1.8 18a2 2 0 0 0 1.7 3h17a2 2 0 0 0 1.7-3L13.7 3.9a2 2 0 0 0-3.4 0z",
        "apps" => "M4 4h6v6H4zM14 4h6v6h-6zM4 14h6v6H4zM14 14h6v6h-6z",
        "clip" => "M9 4h6v3H9zM8 5H6a2 2 0 0 0-2 2v12a2 2 0 0 0 2 2h12a2 2 0 0 0 2-2V7a2 2 0 0 0-2-2h-2M9 13h6M9 17h4",
        "bell" => "M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9M10.3 21a1.94 1.94 0 0 0 3.4 0",
        "camera" => "M14.5 4h-5L7 7H4a2 2 0 0 0-2 2v9a2 2 0 0 0 2 2h16a2 2 0 0 0 2-2V9a2 2 0 0 0-2-2h-3zM12 10a3 3 0 1 0 0 6a3 3 0 1 0 0-6z",
        "record" => "M12 3a9 9 0 1 0 0 18a9 9 0 1 0 0-18zM12 9a3 3 0 1 0 0 6a3 3 0 1 0 0-6z",
        "sound" => "M11 5 6 9H2v6h4l5 4zM15.5 8.5a5 5 0 0 1 0 7M19 5a10 10 0 0 1 0 14",
        "mute" => "M11 5 6 9H2v6h4l5 4zM22 9l-6 6M16 9l6 6",
        "phone" => "M7 2h10a2 2 0 0 1 2 2v16a2 2 0 0 1-2 2H7a2 2 0 0 1-2-2V4a2 2 0 0 1 2-2zM11 18h2",
        "bt" => "M7 7l10 10-5 5V2l5 5L7 17",
        "wifi" => "M5 12.5a10 10 0 0 1 14 0M8.5 16a5 5 0 0 1 7 0M2 9a15 15 0 0 1 20 0M12 20h.01",
        "moon" => "M21 12.8A9 9 0 1 1 11.2 3a7 7 0 0 0 9.8 9.8z",
        "home" => "M12 6a6 6 0 1 0 0 12a6 6 0 1 0 0-12z",
        _ => "",
    }
}

#[component]
pub fn Icon(name: String) -> Element {
    rsx! {
        svg {
            class: "i",
            view_box: "0 0 24 24",
            "aria-hidden": "true",
            path { d: path(&name) }
        }
    }
}
