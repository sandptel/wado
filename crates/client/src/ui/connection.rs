//! Connection group: how the client reaches the server at all.
//!
//! First because nothing else matters if this is wrong, and separate from Session because it
//! is the one group whose value is the same across every session you will ever start.

use dioxus::prelude::*;

use crate::state::Ui;

pub fn render(ui: Ui) -> Element {
    let mut s = ui.set;
    let relay = (s.conn_mode)() == "relay";
    rsx! {
        section { class: "group flat",
            label { "Connection" }
            select {
                value: "{(s.conn_mode)()}",
                onchange: move |e| s.conn_mode.set(e.value()),
                option { value: "direct", "Direct (HTTP — same LAN / port-forward)" }
                option { value: "relay", "Via relay (internet, no port-forward)" }
            }

            if relay {
                label { "Relay URL" }
                input {
                    r#type: "text",
                    placeholder: "ws://my-vps:4000",
                    value: "{(s.relay_url)()}",
                    oninput: move |e| s.relay_url.set(e.value()),
                }
                label { "Remote ID" }
                input {
                    r#type: "text",
                    placeholder: "528-491-307 (shown in the server log)",
                    value: "{(s.remote_id)()}",
                    oninput: move |e| s.remote_id.set(e.value()),
                }
            } else {
                label { "Server" }
                input {
                    r#type: "text",
                    value: "{(s.server_addr)()}",
                    oninput: move |e| s.server_addr.set(e.value()),
                }
            }
        }
    }
}
