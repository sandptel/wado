//! Live check for the tap-target query against a real compositor session — Linux only; the
//! probe itself is `linux/targets_probe.rs`.
#[cfg(target_os = "linux")]
include!("linux/targets_probe.rs");

#[cfg(not(target_os = "linux"))]
fn main() {
    eprintln!("targets_probe needs the Linux compositor");
}
