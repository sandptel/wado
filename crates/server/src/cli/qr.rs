//! `wado qr [--relay URL] [--id ID]` — the connect link and a QR code of it, in the terminal.
//!
//! Scanning it opens the web client with the relay and Remote ID filled in; a device that has
//! never seen this computer adds it, one that has is pointed at it again. Each code carries a
//! single-use pairing code (a day's validity) that trusts the device that scans it. Defaults come from
//! the config (`server { public-relay }`) and the saved Remote ID.

use qrcode::{QrCode, render::unicode};

/// The link a phone opens to connect: the client, told where the relay is and which computer.
pub fn link(client: &str, relay: &str, id: &str) -> String {
    let id: String = id.chars().filter(char::is_ascii_digit).collect();
    format!("{}?relay={}&id={id}", client, relay)
}

/// The link as a QR code drawn with half-blocks, two rows per line, light on dark terminals.
pub fn render(link: &str) -> Option<String> {
    let code = QrCode::new(link.as_bytes()).ok()?;
    Some(
        code.render::<unicode::Dense1x2>()
            .dark_color(unicode::Dense1x2::Light)
            .light_color(unicode::Dense1x2::Dark)
            .quiet_zone(true)
            .build(),
    )
}

/// Print link and code for this computer, or say what is missing.
pub fn print(relay: Option<&str>, id: Option<&str>) -> bool {
    let cfg = wado_config::live::current();
    let relay = relay
        .map(str::to_string)
        .or_else(|| cfg.server.public_relay.clone());
    let id = id
        .map(str::to_string)
        .or_else(|| cfg.server.remote_id.clone())
        .or_else(saved_id);
    let (Some(relay), Some(id)) = (relay, id) else {
        eprintln!(
            "no public relay URL to put in the link — pass --relay https://…, or set \
             server {{ public-relay \"…\" }} in config.kdl (WADO_PUBLIC_RELAY)"
        );
        return false;
    };
    // A fresh single-use pairing code: whoever scans this was shown this computer's screen,
    // so their device is trusted without another device approving it.
    let pair = crate::gate::Gate::default().mint_pair();
    let l = format!("{}&pair={pair}", link(&cfg.server.client_url, &relay, &id));
    if let Some(q) = render(&l) {
        println!("{q}");
    }
    println!("  Scan to connect — or open: {l}\n");
    true
}

fn saved_id() -> Option<String> {
    std::fs::read_to_string(wado_config::paths::config_dir().join("remote_id"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

pub fn run(args: &[String]) -> i32 {
    let mut relay = None;
    let mut id = None;
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--relay" => relay = it.next().cloned(),
            "--id" => id = it.next().cloned(),
            _ => {}
        }
    }
    i32::from(!print(relay.as_deref(), id.as_deref()))
}

#[cfg(test)]
mod tests {
    #[test]
    fn link_and_code() {
        let l = super::link(
            "https://c.example/wado/",
            "https://r.example",
            "872-990-894",
        );
        assert_eq!(
            l,
            "https://c.example/wado/?relay=https://r.example&id=872990894"
        );
        assert!(super::render(&l).is_some_and(|q| q.lines().count() > 10));
    }
}
