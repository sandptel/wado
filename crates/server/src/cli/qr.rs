//! `wado pair [--relay URL] [--id ID] [<grant>…]` (or `wado qr`) — a pairing code for a new
//! device, in the terminal: a QR of the connect link, the same code to type, and how long it has.
//!
//! Scanning it opens the web client with the relay and Remote ID filled in; a device that has
//! never seen this computer adds it, one that has is pointed at it again. On a desktop browser,
//! or when scanning is awkward, the typed code goes into the device's "Pair this device" form.
//! Either one trusts the device with the grants named (its checklist), once, for ten minutes;
//! the QR also pins this computer's identity key. Defaults come from the config
//! (`server { public-relay }`, `security { new-device }`), the rig's tunnel and the saved
//! Remote ID.

use qrcode::{QrCode, render::unicode};

use crate::gate::{Gate, Minted, typed_code};

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

/// A fresh pairing code and the connect link that carries it.
pub struct Pairing {
    pub link: String,
    pub code: Minted,
}

/// A pairing link for this computer whose code gives the device that redeems it `grants`.
///
/// The relay is `relay`, else `server { public-relay }` or the dialled relay, else the rig's
/// live tunnel.
pub fn pair_link(
    relay: Option<&str>,
    id: Option<&str>,
    grants: &crate::gate::Grants,
) -> Result<Pairing, String> {
    let cfg = wado_config::live::current();
    let relay = relay
        .map(str::to_string)
        .or_else(|| cfg.server.public_url())
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
    // so their device is trusted.
    //
    // `hk` pins this computer's identity key on the device that scans it, so a relay can never
    // stand in for this computer to it (`crate::e2e`). Neither the code nor the pin reach the
    // relay: the client keeps both and proves the code inside the envelope.
    let hk = crate::e2e::host_key::HostKey::load_or_create(&crate::remote_id::config_dir())
        .map_err(|e| format!("could not read this computer's identity key: {e}"))?
        .pin();
    let code = Gate::default().mint_pair_with(grants);
    Ok(Pairing {
        link: format!(
            "{}&pair={}&hk={hk}",
            link(&cfg.server.client_url, &relay, &id),
            code.code
        ),
        code,
    })
}

/// Print a QR and a typed code for this computer, or say what is missing. `None` on failure.
fn print_with(
    relay: Option<&str>,
    id: Option<&str>,
    grants: &crate::gate::Grants,
) -> Option<Minted> {
    let p = match pair_link(relay, id, grants) {
        Ok(p) => p,
        Err(e) => {
            eprintln!("{e}");
            return None;
        }
    };
    if let Some(q) = render(&p.link) {
        println!("{q}");
    }
    println!(
        "  Scan to pair — the device may: {}",
        super::devices::describe(grants)
    );
    println!(
        "  or type this code on it: {}",
        super::style::bold(&typed_code::display(&p.code.typed))
    );
    println!("  or open: {}\n", p.link);
    Some(p.code)
}

fn saved_id() -> Option<String> {
    std::fs::read_to_string(wado_config::paths::config_dir().join("remote_id"))
        .ok()
        .map(|s| s.trim().to_string())
        .filter(|s| !s.is_empty())
}

/// `wado pair [--relay URL] [--id ID] [--wait] [<grant>…]` — no grants named: `security {
/// new-device }`. `--wait`: keep going when piped, answering the check words from stdin.
pub fn run(args: &[String]) -> i32 {
    let mut relay = None;
    let mut id = None;
    let mut wait = false;
    let mut tokens = Vec::new();
    let mut it = args.iter();
    while let Some(a) = it.next() {
        match a.as_str() {
            "--relay" => relay = it.next().cloned(),
            "--id" => id = it.next().cloned(),
            "--wait" => wait = true,
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
    match print_with(relay.as_deref(), id.as_deref(), &grants) {
        Some(code) => {
            super::countdown::run(&code, wait);
            0
        }
        None => 1,
    }
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
