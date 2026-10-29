//! The Computer page's hero: the computer's disc and name (the shared element the card grows
//! into), what it runs, and a strip of three readings — how this phone reaches it, whether it
//! is up, and what sessions it holds. The Link reading unfolds into plain words on a tap; while
//! it is being dialled the four hops show under the strip.

use dioxus::prelude::*;

use crate::{
    reach::Reach,
    state::Ui,
    ui::{computer::hops, widgets::Icon},
};

/// "3 d 4 h", "52 min".
fn uptime(s: u64) -> String {
    let (d, h, m) = (s / 86_400, s % 86_400 / 3600, s % 3600 / 60);
    match (d, h) {
        (0, 0) => format!("{m} min"),
        (0, _) => format!("{h} h {m} min"),
        _ => format!("{d} d {h} h"),
    }
}

#[component]
pub fn Hero(ui: Ui, pk: String, r: Reach) -> Element {
    let live = ui.live;
    let mut open = use_signal(|| false);
    let p = crate::route::index_of(ui, &pk)
        .and_then(|i| ui.set.profiles.read().get(i).cloned().map(|p| (i, p)));
    let (name, hue, icon, os) = match &p {
        Some((i, p)) => (
            p.display(),
            crate::profile::hue(*i),
            crate::profile::icon(p),
            [p.os.as_str(), p.desktop.as_str()]
                .into_iter()
                .filter(|s| !s.is_empty())
                .collect::<Vec<_>>()
                .join(" · "),
        ),
        None => (
            crate::profile::default_name(ui),
            "0D",
            "monitor",
            String::new(),
        ),
    };
    let os = match r.host.as_ref().map(|h| &h.info) {
        Some(i) if !i.os.is_empty() => [i.os.as_str(), i.desktop.as_str()]
            .into_iter()
            .filter(|s| !s.is_empty())
            .collect::<Vec<_>>()
            .join(" · "),
        _ if os.is_empty() => p.as_ref().map(|(_, p)| p.address()).unwrap_or_default(),
        _ => os,
    };

    // ── Link ──
    let link = (live.link)();
    let session_list = (live.sessions)().1;
    let (ltone, lval, lms, lsub, explain): (&str, &str, Option<u32>, String, String) =
        if !r.online && r.tone == "bad" {
            (
                "bad",
                "None",
                None,
                "nothing to reach".into(),
                String::new(),
            )
        } else {
            match link.state.as_str() {
                "direct" => (
                    "ok",
                    "Direct",
                    link.rtt_ms,
                    "round trip, now".into(),
                    link.detail(),
                ),
                "ready" => ("ok", "Ready", None, "relay joined".into(), link.detail()),
                "relayed" => (
                    "warn",
                    "Relayed",
                    None,
                    "limited · tap for why".into(),
                    link.detail(),
                ),
                "waiting" => (
                    "warn",
                    "Waiting",
                    None,
                    if r.busy {
                        "in use elsewhere".into()
                    } else {
                        "held by the relay".into()
                    },
                    link.detail(),
                ),
                "connecting" | "reconnecting" | "" => (
                    "warn",
                    "Connecting…",
                    None,
                    "setting up".into(),
                    String::new(),
                ),
                _ => (
                    "bad",
                    "None",
                    None,
                    "nothing to reach".into(),
                    link.detail(),
                ),
            }
        };
    let ldot = match ltone {
        "ok" => "lstate ok",
        "warn" => "lstate warn",
        _ => "lstate bad",
    };

    // ── Computer ──
    let (ctone, cval, csub) = if r.online {
        let up = r.host.as_ref().map(|h| h.info.uptime_s).filter(|s| *s > 0);
        (
            "ok",
            "Online",
            up.map(|s| format!("up {}", uptime(s)))
                .unwrap_or_else(|| "answering".into()),
        )
    } else if r.tone == "bad" {
        (
            "bad",
            "Offline",
            if r.seen.is_empty() {
                "not seen yet".to_string()
            } else {
                format!("seen {}", r.seen)
            },
        )
    } else {
        ("", "…", "asking".to_string())
    };

    // ── Sessions ──
    let (sval, ssub) = if !r.online {
        ("—".to_string(), String::new())
    } else if session_list.is_empty() {
        ("0".to_string(), "none running".to_string())
    } else {
        let first = session_list
            .iter()
            .flat_map(|s| s.apps.first())
            .next()
            .map(|a| crate::ui::home::sessions::short(a))
            .unwrap_or_else(|| "empty desktop".into());
        (session_list.len().to_string(), first)
    };
    let can_explain = !explain.is_empty();

    rsx! {
        section { class: "hero2 sx o1", style: "--n:0", "aria-label": "{name}",
            div { class: "herobg", div { class: "wash" } }
            div { class: "hrow",
                span { class: "disc", style: "--hue:var(--base{hue})", Icon { name: icon } }
                div { class: "htext",
                    h1 { class: "hname", "{name}" }
                    div { class: "hsub", "{os}" }
                }
            }
            div { class: "hmeta",
                button {
                    class: "hcell tone-{ltone}",
                    disabled: !can_explain,
                    "aria-expanded": "{open()}",
                    onclick: move |_| open.toggle(),
                    span { class: "k", "Link" }
                    span { class: "v",
                        span { class: "{ldot}", i { class: "ldot" } }
                        "{lval}"
                        if let Some(ms) = lms {
                            small { span { key: "{ms}", class: "ms roll", "{ms}" } " ms" }
                        }
                    }
                    span { class: "s", "{lsub}" }
                }
                div { class: "hcell tone-{ctone}",
                    span { class: "k", "Computer" }
                    span { class: "v", "{cval}" }
                    span { class: "s", "{csub}" }
                }
                div { class: "hcell",
                    span { class: "k", "Sessions" }
                    span { class: "v", "{sval}" }
                    span { class: "s", "{ssub}" }
                }
            }
            if can_explain {
                div { class: if open() { "explain open" } else { "explain" },
                    p { "{explain}" }
                }
            }
            {hops::render(ui)}
        }
    }
}
