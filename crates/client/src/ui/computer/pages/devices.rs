//! Devices (privileged, `2026-11-04`; the app's `ui/pages/devices.dart`): every device the
//! computer trusts — what it may do, how long it has, whether this device let it in — with Sign
//! out on those; and Add a device: an ordinary pairing code from the computer, capped to this
//! device's own grants and seven days, behind the passkey. This device then answers the new
//! one's check words. The web shows the typed code and the link (the app also draws the QR).

use dioxus::prelude::*;

use crate::{bridge, state::Ui};

fn send(v: serde_json::Value) {
    bridge::call(format!("window.__wado.devicesSend({v});"));
}

fn left(until: Option<u64>) -> String {
    let Some(u) = until else {
        return "until removed".into();
    };
    let s = u.saturating_sub(crate::profile::now_s());
    match s {
        0 => "ending".into(),
        s if s < 3600 => format!("ends in {} min", s.div_ceil(60)),
        s if s < 86_400 => format!("ends in {} h", s / 3600),
        s => format!("ends in {} d", s / 86_400),
    }
}

pub fn render(ui: Ui) -> Element {
    let live = ui.live;
    // Polled while open: a question waits two minutes, and arrives unasked.
    use_future(move || async move {
        loop {
            send(serde_json::json!({ "type": "devices_request" }));
            bridge::sleep(2000).await;
        }
    });
    let mut adding = use_signal(|| false);
    let mut want = use_signal(|| vec!["clipboard".to_string()]);
    let mut secs = use_signal(|| 86_400u64);
    let (rows, asks, err) = (live.devices)();
    let mine = (live.host)().grants.unwrap_or_default();
    // Plain bools, so the check is `Copy` and every handler can hold its own.
    let g = (
        mine.files != "none",
        mine.clipboard,
        mine.join,
        mine.input,
        mine.host,
        mine.shells,
    );
    let has = move |t: &str| match t {
        "files-ro" => g.0,
        "clipboard" => g.1,
        "join" => g.2,
        "input" => g.3,
        "host" => g.4,
        "shells" => g.5,
        _ => false,
    };
    let minted = (live.minted)();
    rsx! {
        p { class: "why", "Every device this computer lets in. This device can sign out the ones it let in; the rest are signed out on the computer." }
        for a in asks.clone() {
            div { key: "{a.typed}", class: "card",
                b { "{a.device} typed your code" }
                p { class: "why", "Let it in only if it shows these same four words." }
                p { class: "mono", "{a.words}" }
                div { class: "grow2",
                    button { class: "btn danger", onclick: { let t = a.typed.clone(); move |_| send(serde_json::json!({ "type": "pair_answer", "typed": t, "yes": false })) }, "They differ" }
                    button { class: "btn primary", onclick: { let t = a.typed.clone(); move |_| send(serde_json::json!({ "type": "pair_answer", "typed": t, "yes": true })) }, "Same — let it in" }
                }
                p { class: "why", "{a.left} s left · silence counts as no" }
            }
        }
        if !err.is_empty() { div { class: "card alert", "{err}" } }
        div { class: "list",
            if rows.is_empty() { div { class: "row", span { class: "why", "Asking the computer…" } } }
            for d in rows.clone() {
                div { key: "{d.key}", class: "row devrow",
                    span { class: "grow1",
                        b { "{d.name}" if d.me { " · this device" } }
                        small { class: "why", "{d.grants} · {left(d.until)}" if d.mine { " · let in by this device" } }
                    }
                    if d.mine {
                        button { class: "btn danger small", onclick: { let k = d.key.clone(); move |_| send(serde_json::json!({ "type": "device_sign_out", "key": k })) }, "Sign out" }
                    }
                }
            }
        }
        if let Some((link, typed, until)) = minted.clone() {
            div { class: "card",
                b { "Code for this computer" }
                p { class: "why", "On the new device, open wado and type this code — or open the link. It works once." }
                p { class: "mono big", "{typed}" }
                button { class: "btn", onclick: { let l = link.clone(); move |_| bridge::call(format!("window.__wado.clipboardToPhone({});", bridge::js(&l))) }, "Copy the link" }
                p { class: "why", "Works until {left(Some(until))}" }
            }
        } else if adding() {
            div { class: "card",
                b { "Let a device in" }
                for (tok, label) in [("clipboard", "Clipboard"), ("files-ro", "Files, read only"), ("join", "Join a session in use"), ("input", "Trackpad and typing"), ("host", "Sound, Bluetooth, sleep"), ("shells", "Shells")] {
                    label { key: "{tok}", class: "grow",
                        input {
                            r#type: "checkbox",
                            disabled: !has(tok),
                            checked: has(tok) && want().iter().any(|w| w == tok),
                            onchange: move |e: FormEvent| { let mut w = want.write(); w.retain(|x| x != tok); if e.checked() { w.push(tok.to_string()); } },
                        }
                        span { "{label}" if !has(tok) { " — this device doesn't have it" } }
                    }
                }
                label { class: "grow", input { r#type: "checkbox", disabled: true } span { "Manage devices — only the computer can give this" } }
                div { class: "seg3",
                    for (s, w) in [(3600u64, "1 hour"), (86_400, "1 day"), (604_800, "7 days")] {
                        button { key: "{s}", class: if secs() == s { "btn primary" } else { "btn" }, onclick: move |_| secs.set(s), "{w}" }
                    }
                }
                p { class: "why", "Up to 7 days from a device. Longer only on the computer." }
                button {
                    class: "btn primary",
                    onclick: move |_| {
                        let grants: Vec<String> = want().into_iter().filter(|t| has(t)).collect();
                        let msg = serde_json::json!({ "type": "pair_mint", "grants": grants, "trust_for_s": secs() });
                        crate::stepup::then("allow", &format!("window.__wado.devicesSend({msg});"));
                    },
                    "Confirm with passkey · make the code"
                }
            }
        } else {
            button { class: "btn primary", onclick: move |_| { let mut m = live.minted; m.set(None); adding.set(true); }, "Add a device" }
        }
    }
}
