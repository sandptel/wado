//! What this Mac is: the Computer page's card. The fixed facts are read once; memory and
//! uptime on every call.

use std::sync::OnceLock;

use wado_platform::sysinfo::SysInfoSource;
use wado_protocol::host::SysInfo;

pub struct MacSysInfo;

/// `sysctlbyname` as a string.
pub fn sysctl_str(name: &str) -> Option<String> {
    let c = std::ffi::CString::new(name).ok()?;
    let mut len = 0usize;
    // SAFETY: the first call asks only for the length; the second fills a buffer that long.
    unsafe {
        if libc::sysctlbyname(
            c.as_ptr(),
            std::ptr::null_mut(),
            &mut len,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        let mut buf = vec![0u8; len];
        if libc::sysctlbyname(
            c.as_ptr(),
            buf.as_mut_ptr().cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        ) != 0
        {
            return None;
        }
        buf.truncate(buf.iter().position(|&b| b == 0).unwrap_or(len));
        String::from_utf8(buf).ok()
    }
}

/// `sysctlbyname` as plain data of type `T`.
fn sysctl<T: Copy + Default>(name: &str) -> Option<T> {
    let c = std::ffi::CString::new(name).ok()?;
    let mut v = T::default();
    let mut len = std::mem::size_of::<T>();
    // SAFETY: `v` is `len` bytes of plain data for the kernel to fill.
    let ok = unsafe {
        libc::sysctlbyname(
            c.as_ptr(),
            (&mut v as *mut T).cast(),
            &mut len,
            std::ptr::null_mut(),
            0,
        )
    } == 0;
    ok.then_some(v)
}

struct Fixed {
    user: String,
    hostname: String,
    os: String,
    kernel: String,
    cpu: String,
    cores: u32,
    mem_total_mb: u64,
    shell: String,
}

fn fixed() -> &'static Fixed {
    static F: OnceLock<Fixed> = OnceLock::new();
    F.get_or_init(|| {
        let hostname = std::process::Command::new("/usr/sbin/scutil")
            .args(["--get", "ComputerName"])
            .output()
            .ok()
            .and_then(|o| String::from_utf8(o.stdout).ok())
            .map(|s| s.trim().to_string())
            .filter(|s| !s.is_empty())
            .or_else(|| sysctl_str("kern.hostname"))
            .unwrap_or_default();
        let version = sysctl_str("kern.osproductversion").unwrap_or_default();
        Fixed {
            user: std::env::var("USER").unwrap_or_default(),
            hostname,
            os: format!("macOS {version}").trim().to_string(),
            kernel: format!(
                "Darwin {}",
                sysctl_str("kern.osrelease").unwrap_or_default()
            ),
            cpu: sysctl_str("machdep.cpu.brand_string").unwrap_or_default(),
            cores: sysctl::<i32>("hw.ncpu").unwrap_or(0).max(0) as u32,
            mem_total_mb: sysctl::<u64>("hw.memsize").unwrap_or(0) / (1024 * 1024),
            shell: std::env::var("SHELL")
                .ok()
                .and_then(|s| s.rsplit('/').next().map(str::to_string))
                .unwrap_or_default(),
        }
    })
}

/// Memory in use, as Activity Monitor counts it: active, wired and compressed pages.
fn mem_used_mb() -> u64 {
    // SAFETY: `vm_statistics64` is plain data; the count is its size in integers, as asked.
    unsafe {
        let mut st: libc::vm_statistics64 = std::mem::zeroed();
        let mut count = (std::mem::size_of::<libc::vm_statistics64>()
            / std::mem::size_of::<libc::integer_t>())
            as libc::mach_msg_type_number_t;
        #[allow(deprecated)]
        let host = libc::mach_host_self();
        if libc::host_statistics64(
            host,
            libc::HOST_VM_INFO64,
            (&mut st as *mut libc::vm_statistics64).cast(),
            &mut count,
        ) != libc::KERN_SUCCESS
        {
            return 0;
        }
        let page = libc::sysconf(libc::_SC_PAGESIZE) as u64;
        let pages = st.active_count as u64 + st.wire_count as u64 + st.compressor_page_count as u64;
        pages * page / (1024 * 1024)
    }
}

fn uptime_s() -> u64 {
    let boot = sysctl::<libc::timeval>("kern.boottime")
        .map(|t| t.tv_sec as u64)
        .unwrap_or(0);
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs())
        .unwrap_or(0);
    now.saturating_sub(boot)
}

impl SysInfoSource for MacSysInfo {
    fn now(&self) -> SysInfo {
        let f = fixed();
        SysInfo {
            user: f.user.clone(),
            hostname: f.hostname.clone(),
            os: f.os.clone(),
            os_id: "macos".into(),
            os_like: Vec::new(),
            kernel: f.kernel.clone(),
            uptime_s: uptime_s(),
            cpu: f.cpu.clone(),
            cores: f.cores,
            // Apple silicon's GPU is the chip's own.
            gpu: if f.cpu.starts_with("Apple") {
                format!("{} GPU", f.cpu)
            } else {
                String::new()
            },
            mem_used_mb: mem_used_mb(),
            mem_total_mb: f.mem_total_mb,
            shell: f.shell.clone(),
            desktop: "Aqua".into(),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sysinfo_reads_macos() {
        let s = MacSysInfo.now();
        assert_eq!(s.os_id, "macos");
        assert!(
            s.os.starts_with("macOS 1") || s.os.starts_with("macOS 2"),
            "{}",
            s.os
        );
        assert!(s.cores > 0 && s.mem_total_mb > 0, "{s:?}");
        assert!(
            s.mem_used_mb > 0 && s.mem_used_mb <= s.mem_total_mb,
            "{s:?}"
        );
        assert!(s.uptime_s > 0 && !s.hostname.is_empty(), "{s:?}");
    }
}
