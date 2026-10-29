//! A thin, raw wrapper over `/dev/uinput`'s modern setup path.
//!
//! ponytail: this duplicates `crates/compositor/src/input/gamepad.rs` almost line for line —
//! same raw `libc::ioctl` approach, same hand-rolled `input_event` struct — but cannot reuse it:
//! the server crate must not depend on the compositor crate (`CLAUDE.md`'s crate boundary), and
//! `gamepad.rs` uses the *legacy* `UI_DEV_SETUP`-free path (one `write()` of `uinput_user_dev`),
//! which has no way to carry an axis `resolution`. The touchpad needs that resolution for
//! `libinput` to compute pointer speed correctly, hence the newer `UI_DEV_SETUP`/`UI_ABS_SETUP`
//! ioctls here instead. If a third virtual device ever needs uinput, the move is one shared
//! micro-crate (`wado-uinput`) that both `gamepad.rs` and this module depend on — not justified
//! for two call sites.

use std::{
    fs::{File, OpenOptions},
    io::{self, Write},
    os::unix::{fs::OpenOptionsExt, io::AsRawFd},
};

pub const EV_SYN: u16 = 0x00;
pub const EV_KEY: u16 = 0x01;
pub const EV_ABS: u16 = 0x03;
pub const SYN_REPORT: u16 = 0x00;

const UI_DEV_SETUP: u64 = 0x405c_5503; // _IOW('U', 3, struct uinput_setup) — 92 bytes
const UI_ABS_SETUP: u64 = 0x401c_5504; // _IOW('U', 4, struct uinput_abs_setup) — 28 bytes
const UI_SET_EVBIT: u64 = 0x4004_5564;
const UI_SET_KEYBIT: u64 = 0x4004_5565;
const UI_SET_ABSBIT: u64 = 0x4004_5567;
const UI_SET_PROPBIT: u64 = 0x4004_556e;
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

/// `struct input_event` on a 64-bit kernel, with a zero timestamp — the kernel timestamps on
/// read, so what this writes is never read back for timing.
#[repr(C)]
struct InputEventRaw {
    sec: i64,
    usec: i64,
    kind: u16,
    code: u16,
    value: i32,
}

/// An open uinput device. Dropping it destroys the kernel device.
pub struct Device {
    file: File,
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
        let dev = Device { file };
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

    /// Write one frame of events, followed by `SYN_REPORT`.
    pub fn emit(&self, events: &[(u16, u16, i32)]) -> io::Result<()> {
        for &(kind, code, value) in events {
            self.write_event(kind, code, value)?;
        }
        self.write_event(EV_SYN, SYN_REPORT, 0)
    }

    fn write_event(&self, kind: u16, code: u16, value: i32) -> io::Result<()> {
        let ev = InputEventRaw {
            sec: 0,
            usec: 0,
            kind,
            code,
            value,
        };
        // Safety: `ev` is a live `#[repr(C)]` value borrowed for the duration of the write.
        let bytes = unsafe {
            std::slice::from_raw_parts(
                (&ev as *const InputEventRaw).cast::<u8>(),
                std::mem::size_of::<InputEventRaw>(),
            )
        };
        (&self.file).write_all(bytes)
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

    /// The ioctl numbers are hand-derived from `_IOW('U', nr, struct)`, which is exactly the
    /// kind of arithmetic that is wrong by a bit and fails as a confusing `EINVAL` at runtime.
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
        assert_eq!(std::mem::size_of::<InputEventRaw>(), 24);
    }
}
