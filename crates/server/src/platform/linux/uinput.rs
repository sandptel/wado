//! A thin, raw wrapper over `/dev/uinput`'s modern setup path.
//!
//! Every batch of events goes to the kernel in **one** `write(2)`: uinput takes any number of
//! `input_event`s per write and injects them in order, so a touch frame of nine events and its
//! `SYN_REPORT` is one syscall, not ten.
//!
//! ponytail: this duplicates `crates/compositor/src/input/gamepad.rs` almost line for line —
//! same raw `libc::ioctl` approach, same hand-rolled `input_event` layout — but `gamepad.rs`
//! uses the *legacy* `UI_DEV_SETUP`-free path (one `write()` of `uinput_user_dev`), which has no
//! way to carry an axis `resolution`. The touchpad needs that resolution for `libinput` to
//! compute pointer speed correctly, hence the newer `UI_DEV_SETUP`/`UI_ABS_SETUP` ioctls here
//! instead. If a third virtual device ever needs uinput, the move is one shared module both
//! depend on — not justified for two call sites.

use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    os::unix::{fs::OpenOptionsExt, io::AsRawFd},
};

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_ABS: u16 = 0x03;
pub const SYN_REPORT: u16 = 0x00;

/// `_IOW(type, nr, size)` — the kernel's ioctl-number encoding (`asm-generic/ioctl.h`):
/// direction (write = 1) in bit 30, the payload size in bits 16..30, the type character in
/// bits 8..16, the sequence number in bits 0..8. Deriving every `_IOW` constant from this
/// (and the real payload struct's `size_of`) instead of a hand-copied hex literal means a
/// wrong size or a transposed nibble fails to compile-time-match the kernel header value
/// pinned in the test below, rather than silently reading a garbage struct at runtime.
const fn iow(nr: u64, size: usize) -> u64 {
    (1 << 30) | ((size as u64) << 16) | (('U' as u64) << 8) | nr
}

const UI_DEV_SETUP: u64 = iow(3, std::mem::size_of::<UinputSetup>());
const UI_ABS_SETUP: u64 = iow(4, std::mem::size_of::<UinputAbsSetup>());
const UI_SET_EVBIT: u64 = iow(100, std::mem::size_of::<i32>());
const UI_SET_KEYBIT: u64 = iow(101, std::mem::size_of::<i32>());
const UI_SET_ABSBIT: u64 = iow(103, std::mem::size_of::<i32>());
const UI_SET_PROPBIT: u64 = iow(110, std::mem::size_of::<i32>());
/// `_IO('U', 1)` / `_IO('U', 2)` — no payload, so not derived from `iow`.
const UI_DEV_CREATE: u64 = 0x5501;
const UI_DEV_DESTROY: u64 = 0x5502;

/// `BUS_VIRTUAL` — every device `Device::open` creates is synthetic, never real hardware.
const BUS_VIRTUAL: u16 = 0x06;

/// `struct input_id` — kernel header `linux/input.h`.
#[repr(C)]
struct InputId {
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
}

/// `struct uinput_setup` — the modern device-identity ioctl payload (92 bytes).
#[repr(C)]
struct UinputSetup {
    id: InputId,
    name: [u8; 80],
    ff_effects_max: u32,
}

/// `struct input_absinfo` — kernel header `linux/input.h`.
#[repr(C)]
#[derive(Default)]
struct InputAbsInfo {
    value: i32,
    minimum: i32,
    maximum: i32,
    fuzz: i32,
    flat: i32,
    resolution: i32,
}

/// `struct uinput_abs_setup` — one axis's range and resolution (28 bytes).
#[repr(C)]
struct UinputAbsSetup {
    code: u16,
    absinfo: InputAbsInfo,
}

/// The size of `struct input_event` on a 64-bit kernel: two `i64` (a zero timestamp — the
/// kernel stamps events itself), `u16` type, `u16` code, `i32` value.
const EVENT_SIZE: usize = 24;

/// Append one `struct input_event` to `buf`, in the kernel's (native) layout.
fn encode(buf: &mut Vec<u8>, kind: u16, code: u16, value: i32) {
    buf.extend_from_slice(&[0; 16]);
    buf.extend_from_slice(&kind.to_ne_bytes());
    buf.extend_from_slice(&code.to_ne_bytes());
    buf.extend_from_slice(&value.to_ne_bytes());
}

/// An open uinput device. Dropping it destroys the kernel device.
pub struct Device {
    file: File,
    /// The bytes of one batch, reused for every batch.
    buf: Vec<u8>,
}

impl Device {
    /// Open `/dev/uinput`, run `setup` to declare the device's capabilities (ev/key/abs/prop
    /// bits and axis ranges), then create it under `name`.
    ///
    /// `setup` runs before `UI_DEV_CREATE`: the kernel refuses capability ioctls once a device
    /// exists.
    pub fn open(name: &str, setup: impl FnOnce(&Device) -> io::Result<()>) -> io::Result<Device> {
        let file = OpenOptions::new()
            .write(true)
            .custom_flags(libc::O_NONBLOCK)
            .open("/dev/uinput")?;
        let dev = Device {
            file,
            buf: Vec::with_capacity(32 * EVENT_SIZE),
        };
        setup(&dev)?;
        dev.create(name)?;
        Ok(dev)
    }

    fn create(&self, name: &str) -> io::Result<()> {
        let mut setup = UinputSetup {
            id: InputId {
                bustype: BUS_VIRTUAL,
                vendor: 0,
                product: 0,
                version: 0,
            },
            name: [0; 80],
            ff_effects_max: 0,
        };
        let bytes = name.as_bytes();
        let n = bytes.len().min(setup.name.len() - 1);
        setup.name[..n].copy_from_slice(&bytes[..n]);

        // Safety: `setup` is a live `#[repr(C)]` value for the duration of the ioctl, and
        // `UI_DEV_SETUP` is documented to read exactly `size_of::<UinputSetup>()` bytes from it.
        unsafe {
            ioctl_ptr(
                self.fd(),
                UI_DEV_SETUP,
                &setup as *const _ as *const libc::c_void,
            )?
        };
        // Safety: `UI_DEV_CREATE` takes no argument.
        if unsafe { libc::ioctl(self.fd(), UI_DEV_CREATE as _) } < 0 {
            return Err(io::Error::last_os_error());
        }
        Ok(())
    }

    /// Declare one `EV_*` event type the device will emit.
    pub fn evbit(&self, code: u16) -> io::Result<()> {
        // Safety: `UI_SET_EVBIT` takes a plain `int` argument.
        unsafe { ioctl_int(self.fd(), UI_SET_EVBIT, code as i32) }
    }

    /// Declare one `KEY_*`/`BTN_*` code the device can report.
    pub fn keybit(&self, code: u16) -> io::Result<()> {
        // Safety: `UI_SET_KEYBIT` takes a plain `int` argument.
        unsafe { ioctl_int(self.fd(), UI_SET_KEYBIT, code as i32) }
    }

    /// Declare one `ABS_*` axis the device can report (the bit, separate from its range).
    pub fn absbit(&self, code: u16) -> io::Result<()> {
        // Safety: `UI_SET_ABSBIT` takes a plain `int` argument.
        unsafe { ioctl_int(self.fd(), UI_SET_ABSBIT, code as i32) }
    }

    /// Declare one `INPUT_PROP_*` flag (e.g. `INPUT_PROP_POINTER`, `INPUT_PROP_BUTTONPAD`).
    pub fn propbit(&self, code: u16) -> io::Result<()> {
        // Safety: `UI_SET_PROPBIT` takes a plain `int` argument.
        unsafe { ioctl_int(self.fd(), UI_SET_PROPBIT, code as i32) }
    }

    /// Set one `ABS_*` axis's range (`0..=max`) and resolution. Does not itself set the bit —
    /// call [`Self::absbit`] too.
    pub fn abs(&self, code: u16, max: i32, res: i32) -> io::Result<()> {
        let setup = UinputAbsSetup {
            code,
            absinfo: InputAbsInfo {
                maximum: max,
                resolution: res,
                ..Default::default()
            },
        };
        // Safety: `setup` is a live `#[repr(C)]` value for the duration of the ioctl, and
        // `UI_ABS_SETUP` is documented to read exactly `size_of::<UinputAbsSetup>()` bytes.
        unsafe {
            ioctl_ptr(
                self.fd(),
                UI_ABS_SETUP,
                &setup as *const _ as *const libc::c_void,
            )
        }
    }

    /// Write one frame of events, followed by `SYN_REPORT` — one `write(2)`.
    pub fn emit(&mut self, events: &[(u16, u16, i32)]) -> io::Result<()> {
        self.buf.clear();
        for &(kind, code, value) in events {
            encode(&mut self.buf, kind, code, value);
        }
        encode(&mut self.buf, EV_SYN, SYN_REPORT, 0);
        (&self.file).write_all(&self.buf)
    }

    /// Write each event as a frame of its own (each followed by `SYN_REPORT`) — a key chord,
    /// whose presses must arrive as separate reports in order — still in one `write(2)`.
    pub fn emit_each(&mut self, events: &[(u16, u16, i32)]) -> io::Result<()> {
        self.buf.clear();
        for &(kind, code, value) in events {
            encode(&mut self.buf, kind, code, value);
            encode(&mut self.buf, EV_SYN, SYN_REPORT, 0);
        }
        (&self.file).write_all(&self.buf)
    }

    fn fd(&self) -> i32 {
        self.file.as_raw_fd()
    }
}

impl Drop for Device {
    fn drop(&mut self) {
        // Safety: the descriptor is still owned and open here; the file closes right after.
        unsafe { libc::ioctl(self.fd(), UI_DEV_DESTROY as _) };
    }
}

/// One setup ioctl taking a plain `int` argument.
///
/// # Safety
/// `fd` must be an open `/dev/uinput` descriptor and `req` one of the `_IOW(…, int)` commands.
unsafe fn ioctl_int(fd: i32, req: u64, arg: i32) -> io::Result<()> {
    if unsafe { libc::ioctl(fd, req as _, arg) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

/// One setup ioctl taking a pointer to a fixed-size struct.
///
/// # Safety
/// `fd` must be an open `/dev/uinput` descriptor, `req` one of the `_IOW(…, struct)` commands,
/// and `arg` must point to a live value of exactly the size that command reads.
unsafe fn ioctl_ptr(fd: i32, req: u64, arg: *const libc::c_void) -> io::Result<()> {
    if unsafe { libc::ioctl(fd, req as _, arg) } < 0 {
        return Err(io::Error::last_os_error());
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// `UI_*` are computed from `iow()` and the real struct sizes, not copied from the kernel
    /// header by hand — so this test is the thing that actually pins them to the header's
    /// values (a wrong `nr`, a wrong `'U'`, or a struct whose size drifted would fail here).
    #[test]
    fn ioctl_numbers_match_the_kernel_headers() {
        assert_eq!(UI_DEV_SETUP, 0x405c_5503);
        assert_eq!(UI_ABS_SETUP, 0x401c_5504);
        assert_eq!(UI_SET_EVBIT, 0x4004_5564);
        assert_eq!(UI_SET_KEYBIT, 0x4004_5565);
        assert_eq!(UI_SET_ABSBIT, 0x4004_5567);
        assert_eq!(UI_SET_PROPBIT, 0x4004_556e);
        assert_eq!(UI_DEV_CREATE, 0x5501);
        assert_eq!(UI_DEV_DESTROY, 0x5502);
    }

    /// The kernel reads a fixed-size struct off the fd; a layout that does not match is read as
    /// garbage (or rejected with `EINVAL`) rather than caught at compile time.
    #[test]
    fn setup_structs_are_the_sizes_the_kernel_expects() {
        assert_eq!(std::mem::size_of::<UinputSetup>(), 92);
        assert_eq!(std::mem::size_of::<UinputAbsSetup>(), 28);
        assert_eq!(std::mem::size_of::<libc::input_event>(), EVENT_SIZE);
    }

    /// The bytes written are exactly the kernel's `struct input_event`.
    #[test]
    fn an_event_is_encoded_as_the_kernel_lays_it_out() {
        let mut buf = Vec::new();
        encode(&mut buf, EV_ABS, 0x35, -7);
        // SAFETY: plain data, and `buf` is exactly one event long.
        let ev: libc::input_event =
            unsafe { std::ptr::read_unaligned(buf.as_ptr().cast::<libc::input_event>()) };
        assert_eq!(buf.len(), EVENT_SIZE);
        assert_eq!((ev.type_, ev.code, ev.value), (EV_ABS, 0x35, -7));
        assert_eq!((ev.time.tv_sec, ev.time.tv_usec), (0, 0));
    }
}
