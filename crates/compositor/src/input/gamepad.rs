//! A virtual gamepad on the host, via `/dev/uinput`.
//!
//! **Why this cannot be a Wayland protocol.** There is no gamepad in Wayland, and there never
//! has been: every game, every engine and SDL itself read controllers straight from
//! `/dev/input/event*` through evdev, below the display server entirely. So a touch gamepad
//! drawn on a phone has exactly one way to reach a game running in the session — a real kernel
//! input device. Synthesising `wl_keyboard` instead is the *other* mode, and it is a different
//! thing: it makes the pad type WASD, which works in games that read a keyboard and is useless
//! in the many that only read a pad.
//!
//! **Why it pretends to be an Xbox 360 controller.** Button and axis codes alone do not make a
//! device recognisable: SDL keys its built-in mapping database on vendor/product, and a pad
//! with an unknown id lands in "unmapped joystick" territory where A and B are wherever the
//! game guesses. Reporting `045e:028e` with the exact `xpad` layout means every game already
//! ships a correct mapping for this device, and no mapping string has to be shipped or pasted
//! anywhere.
//!
//! **Scope, said out loud: this device is global to the machine.** A uinput device is a kernel
//! input device; it is visible to everything that can read `/dev/input`, inside the wado
//! session and outside it. That is deliberate — it is what makes the pad work for a game
//! launched however it was launched — but it is also why the client calls this mode "global"
//! and offers a key-mapping mode that stays inside the session. The device is created on first
//! use and destroyed with the session.
//!
//! ponytail: raw `libc` ioctls and the legacy `uinput_user_dev` setup path, rather than a
//! crate. `libc` is already a dependency (it is what `proc.rs` kills process groups with), the
//! legacy path is one struct write instead of an `UI_ABS_SETUP` per axis, and it works on every
//! kernel since 2.6. ~120 lines against a new dependency on a tree that pins everything
//! deliberately.

use std::{
    fs::{File, OpenOptions},
    io::Write,
    os::unix::io::AsRawFd,
};

/// `_IOW('U', nr, int)` — every uinput setup ioctl has this shape.
const fn uinput_iow(nr: u64) -> u64 {
    0x4004_5500 | nr
}
const UI_SET_EVBIT: u64 = uinput_iow(100);
const UI_SET_KEYBIT: u64 = uinput_iow(101);
const UI_SET_ABSBIT: u64 = uinput_iow(103);
/// `_IO('U', 1)` / `_IO('U', 2)`.
const UI_DEV_CREATE: u64 = 0x5501;
const UI_DEV_DESTROY: u64 = 0x5502;

const EV_SYN: u16 = 0x00;
const EV_KEY: u16 = 0x01;
const EV_ABS: u16 = 0x03;
const SYN_REPORT: u16 = 0x00;

/// The `xpad` button set, in the order the driver reports it.
const BUTTONS: [u16; 11] = [
    0x130, // BTN_A
    0x131, // BTN_B
    0x133, // BTN_X
    0x134, // BTN_Y
    0x136, // BTN_TL
    0x137, // BTN_TR
    0x13a, // BTN_SELECT
    0x13b, // BTN_START
    0x13c, // BTN_MODE
    0x13d, // BTN_THUMBL
    0x13e, // BTN_THUMBR
];

/// The `xpad` axis set as `(code, min, max)`. Sticks are signed 16-bit, triggers are 0..255,
/// and the hat is the three-state -1/0/1 that a D-pad reports.
const AXES: [(u16, i32, i32); 8] = [
    (0x00, -32768, 32767), // ABS_X
    (0x01, -32768, 32767), // ABS_Y
    (0x02, 0, 255),        // ABS_Z   — left trigger
    (0x03, -32768, 32767), // ABS_RX
    (0x04, -32768, 32767), // ABS_RY
    (0x05, 0, 255),        // ABS_RZ  — right trigger
    (0x10, -1, 1),         // ABS_HAT0X
    (0x11, -1, 1),         // ABS_HAT0Y
];

/// `struct input_event` on a 64-bit kernel. Declared here rather than taken from `libc` so the
/// layout this writes is visible next to the write that depends on it.
#[repr(C)]
struct InputEventRaw {
    sec: i64,
    usec: i64,
    kind: u16,
    code: u16,
    value: i32,
}

/// `struct uinput_user_dev` — the legacy one-shot device description.
#[repr(C)]
struct UinputUserDev {
    name: [u8; 80],
    bustype: u16,
    vendor: u16,
    product: u16,
    version: u16,
    ff_effects_max: u32,
    absmax: [i32; 64],
    absmin: [i32; 64],
    absfuzz: [i32; 64],
    absflat: [i32; 64],
}

/// An open virtual gamepad. Dropping it destroys the kernel device.
pub struct Gamepad {
    file: File,
}

impl Gamepad {
    /// Create the device, or say why not.
    ///
    /// The error is a plain string because it has exactly one destination: the log the viewer
    /// reads. `EACCES` here is not a bug and is by far the likeliest outcome on a fresh
    /// machine — `/dev/uinput` is `root:uinput 0660` — so the message names the fix rather
    /// than the errno.
    pub fn open() -> Result<Self, String> {
        let file = OpenOptions::new()
            .write(true)
            .open("/dev/uinput")
            .map_err(|e| match e.kind() {
                std::io::ErrorKind::PermissionDenied => "/dev/uinput: permission denied — add \
                     this user to the `uinput` group (NixOS: hardware.uinput.enable = true) \
                     and log in again"
                    .to_string(),
                std::io::ErrorKind::NotFound => {
                    "/dev/uinput is missing — the `uinput` kernel module is not loaded".to_string()
                }
                _ => format!("/dev/uinput: {e}"),
            })?;
        let fd = file.as_raw_fd();

        // Safety: every call is an ioctl on a file descriptor this function owns, with the
        // integer argument each of these three commands is defined to take.
        unsafe {
            set(fd, UI_SET_EVBIT, EV_KEY as i32)?;
            set(fd, UI_SET_EVBIT, EV_ABS as i32)?;
            for b in BUTTONS {
                set(fd, UI_SET_KEYBIT, b as i32)?;
            }
            for (code, _, _) in AXES {
                set(fd, UI_SET_ABSBIT, code as i32)?;
            }
        }

        let mut dev = UinputUserDev {
            name: [0; 80],
            // Microsoft X-Box 360 pad, on the virtual bus. The ids are what SDL matches on;
            // the bus is honest about what this is.
            bustype: 0x06, // BUS_VIRTUAL
            vendor: 0x045e,
            product: 0x028e,
            version: 0x0110,
            ff_effects_max: 0,
            absmax: [0; 64],
            absmin: [0; 64],
            absfuzz: [0; 64],
            absflat: [0; 64],
        };
        let name = b"wado virtual gamepad";
        dev.name[..name.len()].copy_from_slice(name);
        for (code, min, max) in AXES {
            dev.absmin[code as usize] = min;
            dev.absmax[code as usize] = max;
        }

        // Safety: `dev` is a live `#[repr(C)]` value and the slice borrows it for the write.
        let bytes = unsafe {
            std::slice::from_raw_parts(
                (&dev as *const UinputUserDev).cast::<u8>(),
                std::mem::size_of::<UinputUserDev>(),
            )
        };
        (&file)
            .write_all(bytes)
            .map_err(|e| format!("uinput device setup: {e}"))?;

        // Safety: same owned descriptor; `UI_DEV_CREATE` takes no argument.
        if unsafe { libc::ioctl(fd, UI_DEV_CREATE as _) } < 0 {
            return Err(format!(
                "UI_DEV_CREATE: {}",
                std::io::Error::last_os_error()
            ));
        }
        tracing::info!("virtual gamepad created (uinput, xbox360-compatible)");
        Ok(Self { file })
    }

    /// Press or release one `BTN_*` code.
    pub fn button(&mut self, code: u16, pressed: bool) {
        self.emit(EV_KEY, code, pressed as i32);
        self.sync();
    }

    /// Move one `ABS_*` axis. The value is clamped to the range the device advertised, because
    /// an out-of-range value is silently dropped by the kernel and looks like a dead stick.
    pub fn axis(&mut self, code: u16, value: i32) {
        let value = match AXES.iter().find(|(c, _, _)| *c == code) {
            Some((_, min, max)) => value.clamp(*min, *max),
            None => {
                tracing::debug!(code, "gamepad axis not on this device — ignored");
                return;
            }
        };
        self.emit(EV_ABS, code, value);
        self.sync();
    }

    fn sync(&mut self) {
        self.emit(EV_SYN, SYN_REPORT, 0);
    }

    /// A failed write is logged once per event and otherwise ignored: the only realistic cause
    /// is the device having gone away, and there is nothing useful to do about it mid-game.
    fn emit(&mut self, kind: u16, code: u16, value: i32) {
        let ev = InputEventRaw { sec: 0, usec: 0, kind, code, value };
        // Safety: `ev` is a live `#[repr(C)]` value borrowed for the duration of the write.
        let bytes = unsafe {
            std::slice::from_raw_parts(
                (&ev as *const InputEventRaw).cast::<u8>(),
                std::mem::size_of::<InputEventRaw>(),
            )
        };
        if let Err(e) = self.file.write_all(bytes) {
            tracing::warn!("virtual gamepad write failed: {e}");
        }
    }
}

impl Drop for Gamepad {
    fn drop(&mut self) {
        // Safety: the descriptor is still owned and open here; the file closes right after.
        unsafe { libc::ioctl(self.file.as_raw_fd(), UI_DEV_DESTROY as _) };
        tracing::info!("virtual gamepad destroyed");
    }
}

/// One setup ioctl, with the errno turned into the message the log wants.
///
/// # Safety
/// `fd` must be an open `/dev/uinput` descriptor and `req` one of the `_IOW(…, int)` commands.
unsafe fn set(fd: i32, req: u64, arg: i32) -> Result<(), String> {
    if unsafe { libc::ioctl(fd, req as _, arg) } < 0 {
        return Err(format!(
            "uinput ioctl {req:#x}({arg:#x}): {}",
            std::io::Error::last_os_error()
        ));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The ioctl numbers are hand-derived from `_IOW('U', nr, int)`, which is exactly the kind
    /// of arithmetic that is wrong by a bit and fails as a confusing `EINVAL` at runtime.
    #[test]
    fn ioctl_numbers_match_the_kernel_headers() {
        assert_eq!(UI_SET_EVBIT, 0x4004_5564);
        assert_eq!(UI_SET_KEYBIT, 0x4004_5565);
        assert_eq!(UI_SET_ABSBIT, 0x4004_5567);
    }

    /// The kernel reads a fixed-size struct off the fd; a layout that does not match is read as
    /// garbage axis ranges rather than rejected.
    #[test]
    fn the_setup_struct_is_the_size_the_kernel_expects() {
        assert_eq!(std::mem::size_of::<UinputUserDev>(), 1116);
        assert_eq!(std::mem::size_of::<InputEventRaw>(), 24);
    }

    /// Both arrays are indexed by evdev code into 64-entry tables, so a code past the end would
    /// be an out-of-bounds write during setup.
    #[test]
    fn every_axis_code_fits_the_abs_tables() {
        for (code, min, max) in AXES {
            assert!(code < 64, "ABS code {code:#x} is past the abs* tables");
            assert!(min < max, "ABS code {code:#x} has an empty range");
        }
    }
}
