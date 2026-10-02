//! The computer, neofetch-style: a logo for its OS beside `user@host` and the facts, with the
//! scheme's colour blocks underneath — shown once the daemon has said who it is.

use dioxus::prelude::*;
use wado_protocol::host::SysInfo;

/// A small logo per OS family, in the scheme's accent. Generic Tux for anything else.
fn logo(id: &str) -> &'static str {
    match id {
        "nixos" => "  \\\\  \\\\ //\n ==\\\\__\\\\/ //\n   //   \\\\//\n==//     //==\n //\\\\___//\n// /\\\\  \\\\==\n  // \\\\  \\\\",
        "arch" | "endeavouros" | "manjaro" => "      /\\\n     /  \\\n    /\\   \\\n   /  __  \\\n  /  (  )  \\\n / __|  |__\\\n/.`        `.\\",
        "ubuntu" | "pop" => "        _\n    ---(_)\n_/  ---  \\\n(_) |   |\n  \\  --- _/\n     ---(_)",
        "debian" => "  _____\n /  __ \\\n|  /    |\n|  \\___-\n-_\n  --_",
        "fedora" => "      _____\n     /   __)\\\n     |  /  \\ \\\n  ___|  |__/ /\n / (_    _)_/\n/ /  |  |\n\\ \\__/  |\n \\(_____/",
        _ => "    .--.\n   |o_o |\n   |:_/ |\n  //   \\ \\\n (|     | )\n/'\\_   _/`\\\n\\___)=(___/",
    }
}

fn uptime(s: u64) -> String {
    let (d, h, m) = (s / 86400, s / 3600 % 24, s / 60 % 60);
    match (d, h) {
        (0, 0) => format!("{m} mins"),
        (0, _) => format!("{h} hours, {m} mins"),
        _ => format!("{d} days, {h} hours"),
    }
}

#[component]
pub fn Fetch(info: SysInfo) -> Element {
    let head = format!(
        "{}@{}",
        if info.user.is_empty() {
            "user"
        } else {
            &info.user
        },
        info.hostname
    );
    let mut rows: Vec<(&str, String)> = vec![
        ("OS", info.os.clone()),
        ("Kernel", info.kernel.clone()),
        ("Uptime", uptime(info.uptime_s)),
        ("Shell", info.shell.clone()),
        ("DE", info.desktop.clone()),
        (
            "CPU",
            if info.cores > 0 {
                format!("{} ({})", info.cpu, info.cores)
            } else {
                info.cpu.clone()
            },
        ),
        ("GPU", info.gpu.clone()),
    ];
    if info.mem_total_mb > 0 {
        rows.push((
            "Memory",
            format!("{} MiB / {} MiB", info.mem_used_mb, info.mem_total_mb),
        ));
    }
    rows.retain(|(_, v)| !v.trim().is_empty());
    let line = "-".repeat(head.chars().count());

    rsx! {
        div { class: "fetch",
            pre { class: "fetchlogo", "{logo(&info.os_id)}" }
            div { class: "fetchtext",
                div { class: "fetchhead", "{head}" }
                div { class: "fetchrule", "{line}" }
                for (k, v) in rows {
                    div { key: "{k}", class: "fetchrow", span { "{k}" } ": {v}" }
                }
                div { class: "fetchblocks",
                    for slot in ["00", "08", "09", "0A", "0B", "0C", "0D", "0E", "0F", "05"] {
                        i { key: "{slot}", style: "background:var(--base{slot})" }
                    }
                }
            }
        }
    }
}
