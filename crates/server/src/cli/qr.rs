//! `wado pair [--relay URL] [--id ID] [<grant>…]` (or `wado qr`) — the connect link and a QR
//! code of it, in the terminal.
//!
//! Scanning it opens the web client with the relay and Remote ID filled in; a device that has
//! never seen this computer adds it, one that has is pointed at it again. Each code carries a
//! single-use pairing code (a day's validity) that trusts the device that scans it with the
//! grants named — its checklist — and the pin of this computer's identity key. Defaults come
//! from the config (`server { public-relay }`, `security { new-device }`), the rig's tunnel and
//! the saved Remote ID.

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

/// A pairing link for this computer whose code gives the device that redeems it `grants`.
///
/// The relay is `relay`, else `server { public-relay }`, else the rig's live tunnel.
pub fn pair_link(
    relay: Option<&str>,
    id: Option<&str>,
    grants: &crate::gate::Grants,
) -> Result<String, String> {
    let cfg = wado_config::live::current();
    let relay = relay
        .map(str::to_string)
        .or_else(|| cfg.server.public_relay.clone())
        .or_else(|| super::status::read().tunnel);
    let id = id
        .map(str::to_string)
        .or_else(|| cfg.server.remote_id.clone())
        .or_else(saved_id);
    let (Some(relay), Some(id)) = (relay, id) else {
        return Err(
            "no public relay URL to put in the link — pass --relay https://…, or set \
             server { public-relay \"…\" } in config.kdl (WADO_PUBLIC_RELAY)"
                .into(),
        );
    };
    // A fresh single-use pairing code: whoever scans this was shown this computer's screen,
    // so their device is trusted without another device approving it.
    //
    // `hk` pins this computer's identity key on the device that scans it, so a relay can never
    // stand in for this computer to it (`crate::e2e`). Neither the code nor the pin reach the
    // relay: the client keeps both and proves the code inside the envelope.
    let hk = crate::e2e::host_key::HostKey::load_or_create(&crate::remote_id::config_dir())
        .map_err(|e| format!("could not read this computer's identity key: {e}"))?
        .pin();
    let pair = crate::gate::Gate::default().mint_pair_with(grants);
    Ok(format!(
        "{}&pair={pair}&hk={hk}",
        link(&cfg.server.client_url, &relay, &id)
    ))
}

/// Print link and code for this computer, or say what is missing.
pub fn print(relay: Option<&str>, id: Option<&str>) -> bool {
    print_with(relay, id, &crate::gate::Grants::new_device())
}

fn print_with(relay: Option<&str>, id: Option<&str>, grants: &crate::gate::Grants) -> bool {
    let l = match pair_link(relay, id, grants) {
        Ok(l) => l,
        Err(e) => {
            eprintln!("{e}");
            return false;
        }
    };
    if let Some(q) = render(&l) {
        println!("{q}");
    }
    println!(
        "  Scan to connect — the device may: {}",
        super::devices::describe(grants)
    );
    println!("  or open: {l}\n");
    true
}

fn saved_id() -> Option<String> {
    std::fs::read_to_string(wado_config::paths::config_dir().join("remote_id"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// `wado pair [--relay URL] [--id ID] [<grant>…]` — no grants named: `security { new-device }`.
pub fn run(args: &[String]) -> i32 {
    let mut relay = None;
    let mut id = None;
    let mut tokens = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--relay" => relay = it.next().cloned(),
            "--id" => id = it.next().cloned(),
            t => tokens.push(t),
        }
    }
    let grants = if tokens.is_empty() {
        crate::gate::Grants::new_device()
    } else {
        match crate::gate::Grants::parse(&tokens.join(" ")) {
            Ok(g) => g,
            Err(e) => {
                eprintln!("{e}");
                return 2;
            }
        }
    };
    i32::from(!print_with(relay.as_deref(), id.as_deref(), &grants))
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
