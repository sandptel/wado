//! What the computer is, neofetch-style: OS, kernel, uptime, CPU, GPU, memory, shell, desktop.
//!
//! All from `/proc`, `/etc/os-release` and `/sys` — nothing to install. What cannot change
//! while the daemon runs is read once; uptime and memory are read on every ask.

use std::sync::OnceLock;

use wado_protocol::host::SysInfo;

struct Fixed {
    user: String,
    hostname: String,
    os: String,
    os_id: String,
    kernel: String,
    cpu: String,
    cores: u32,
    gpu: String,
    shell: String,
    desktop: String,
}

fn read(path: &str) -> String {
    std::fs::read_to_string(path).unwrap_or_default()
}

fn fixed() -> &'static Fixed {
    static F: OnceLock<Fixed> = OnceLock::new();
    F.get_or_init(|| {
        let release = read("/etc/os-release");
        let field = |k: &str| {
            release
                .lines()
                .find_map(|l| l.strip_prefix(&format!("{k}=")))
                .map(|v| v.trim_matches('"').to_string())
                .unwrap_or_default()
        };
        let cpuinfo = read("/proc/cpuinfo");
        let cpu = cpuinfo
            .lines()
            .find_map(|l| {
                l.strip_prefix("model name")
                    .and_then(|r| r.split_once(':'))
                    .map(|(_, v)| v.trim().to_string())
            })
            .unwrap_or_default();
        let cores = cpuinfo
            .lines()
            .filter(|l| l.starts_with("processor"))
            .count() as u32;
        let env = |k: &str| std::env::var(k).unwrap_or_default();
        Fixed {
            user: env("USER"),
            hostname: read("/proc/sys/kernel/hostname").trim().to_string(),
            os: if field("PRETTY_NAME").is_empty() {
                field("NAME")
            } else {
                field("PRETTY_NAME")
            },
            os_id: field("ID"),
            kernel: read("/proc/sys/kernel/osrelease").trim().to_string(),
            cpu,
            cores,
            gpu: gpu(),
            shell: env("SHELL").rsplit('/').next().unwrap_or("").to_string(),
            desktop: [env("XDG_CURRENT_DESKTOP"), env("DESKTOP_SESSION")]
                .into_iter()
                .find(|s| !s.is_empty())
                .unwrap_or_default(),
        }
    })
}

/// The render node's GPU, named by `lspci` when it is there, else by its kernel driver.
fn gpu() -> String {
    let uevent = read("/sys/class/drm/renderD128/device/uevent");
    let slot = uevent
        .lines()
        .find_map(|l| l.strip_prefix("PCI_SLOT_NAME="))
        .unwrap_or("")
        .to_string();
    let driver = uevent
        .lines()
        .find_map(|l| l.strip_prefix("DRIVER="))
        .unwrap_or("")
        .to_string();
    if !slot.is_empty() {
        if let Ok(out) = std::process::Command::new("lspci")
            .args(["-mm", "-s", &slot])
            .output()
        {
            // `62:00.0 "Display controller" "Vendor" "Device" …` — the vendor and device fields.
            let line = String::from_utf8_lossy(&out.stdout);
            let quoted: Vec<&str> = line.split('"').skip(1).step_by(2).collect();
            if let (Some(v), Some(d)) = (quoted.get(1), quoted.get(2)) {
                let v = v
                    .replace("Advanced Micro Devices, Inc. [AMD/ATI]", "AMD")
                    .replace("Intel Corporation", "Intel")
                    .replace("NVIDIA Corporation", "NVIDIA");
                return format!("{v} {d}");
            }
        }
    }
    driver
}

pub fn now() -> SysInfo {
    let f = fixed();
    let uptime_s = read("/proc/uptime")
        .split_whitespace()
        .next()
        .and_then(|s| s.parse::<f64>().ok())
        .unwrap_or(0.0) as u64;
    let meminfo = read("/proc/meminfo");
    let kb = |k: &str| {
        meminfo
            .lines()
            .find_map(|l| l.strip_prefix(k))
            .and_then(|r| r.split_whitespace().next())
            .and_then(|v| v.parse::<u64>().ok())
            .unwrap_or(0)
    };
    let total = kb("MemTotal:");
    SysInfo {
        user: f.user.clone(),
        hostname: f.hostname.clone(),
        os: f.os.clone(),
        os_id: f.os_id.clone(),
        kernel: f.kernel.clone(),
        uptime_s,
        cpu: f.cpu.clone(),
        cores: f.cores,
        gpu: f.gpu.clone(),
        mem_used_mb: total.saturating_sub(kb("MemAvailable:")) / 1024,
        mem_total_mb: total / 1024,
        shell: f.shell.clone(),
        desktop: f.desktop.clone(),
    }
}
