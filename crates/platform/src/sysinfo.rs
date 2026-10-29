//! What the computer is, neofetch-style.

use wado_protocol::host::SysInfo;

/// OS, kernel, uptime, CPU, GPU, memory, shell, desktop. Cheap enough to read on every ask: what
/// cannot change is read once, uptime and memory each time.
pub trait SysInfoSource: Send + Sync {
    fn now(&self) -> SysInfo;
}
