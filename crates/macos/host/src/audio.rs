//! This Mac's sound outputs through CoreAudio: each output device's volume and mute, and which
//! one plays. macOS has no public per-app volume, so streams are always empty here.

use std::ffi::c_void;
use std::time::Duration;

use objc2_core_foundation::CFString;
use tokio::sync::watch;
use wado_platform::audio::AudioMixer;
use wado_platform::state::Published;
use wado_platform::{Fut, Res};
use wado_protocol::host::{HostAudio, Sink};

#[repr(C)]
struct Address {
    selector: u32,
    scope: u32,
    element: u32,
}

#[link(name = "CoreAudio", kind = "framework")]
unsafe extern "C" {
    fn AudioObjectGetPropertyDataSize(
        id: u32,
        a: *const Address,
        ql: u32,
        q: *const c_void,
        size: *mut u32,
    ) -> i32;
    fn AudioObjectGetPropertyData(
        id: u32,
        a: *const Address,
        ql: u32,
        q: *const c_void,
        size: *mut u32,
        out: *mut c_void,
    ) -> i32;
    fn AudioObjectSetPropertyData(
        id: u32,
        a: *const Address,
        ql: u32,
        q: *const c_void,
        size: u32,
        data: *const c_void,
    ) -> i32;
    fn AudioObjectHasProperty(id: u32, a: *const Address) -> bool;
}

const fn cc(s: &[u8; 4]) -> u32 {
    u32::from_be_bytes(*s)
}

const SYSTEM: u32 = 1;
const GLOBAL: u32 = cc(b"glob");
const OUTPUT: u32 = cc(b"outp");
const DEVICES: u32 = cc(b"dev#");
const DEFAULT_OUT: u32 = cc(b"dOut");
const STREAMS: u32 = cc(b"stm#");
const NAME: u32 = cc(b"lnam");
const UID: u32 = cc(b"uid ");
const VIRTUAL_VOLUME: u32 = cc(b"vmvc");
const MUTE: u32 = cc(b"mute");

/// What is not on a Mac.
pub const NO_STREAMS: &str = "Not on macOS: per-app sound needs a virtual audio driver.";

fn addr(selector: u32, scope: u32) -> Address {
    Address {
        selector,
        scope,
        element: 0,
    }
}

fn get<T: Copy + Default>(id: u32, a: Address) -> Option<T> {
    let mut v = T::default();
    let mut size = std::mem::size_of::<T>() as u32;
    // SAFETY: `v` is `size` bytes of plain data for CoreAudio to fill.
    (unsafe {
        AudioObjectGetPropertyData(
            id,
            &a,
            0,
            std::ptr::null(),
            &mut size,
            (&mut v as *mut T).cast(),
        )
    } == 0)
        .then_some(v)
}

fn set<T: Copy>(id: u32, a: Address, v: T) -> Res<()> {
    // SAFETY: `v` is plain data of the property's type.
    let r = unsafe {
        AudioObjectSetPropertyData(
            id,
            &a,
            0,
            std::ptr::null(),
            std::mem::size_of::<T>() as u32,
            (&v as *const T).cast(),
        )
    };
    if r == 0 {
        Ok(())
    } else {
        Err(format!("macOS refused the sound change ({r})"))
    }
}

fn string(id: u32, a: Address) -> Option<String> {
    let p: *const CFString = get(id, a)?;
    if p.is_null() {
        return None;
    }
    // SAFETY: CoreAudio returns a +1 CFString for these properties; we own it.
    let s = unsafe {
        objc2_core_foundation::CFRetained::from_raw(std::ptr::NonNull::new(p as *mut CFString)?)
    };
    Some(s.to_string())
}

/// Every device that can play sound.
fn outputs() -> Vec<u32> {
    let a = addr(DEVICES, GLOBAL);
    let mut size = 0u32;
    // SAFETY: asks only for the size.
    if unsafe { AudioObjectGetPropertyDataSize(SYSTEM, &a, 0, std::ptr::null(), &mut size) } != 0 {
        return Vec::new();
    }
    let mut ids = vec![0u32; size as usize / 4];
    // SAFETY: a buffer of exactly `size` bytes.
    if unsafe {
        AudioObjectGetPropertyData(
            SYSTEM,
            &a,
            0,
            std::ptr::null(),
            &mut size,
            ids.as_mut_ptr().cast(),
        )
    } != 0
    {
        return Vec::new();
    }
    ids.into_iter()
        .filter(|&id| {
            let mut s = 0u32;
            // SAFETY: asks only for the size of its output streams.
            unsafe {
                AudioObjectGetPropertyDataSize(
                    id,
                    &addr(STREAMS, OUTPUT),
                    0,
                    std::ptr::null(),
                    &mut s,
                ) == 0
                    && s > 0
            }
        })
        .collect()
}

fn has(id: u32, a: Address) -> bool {
    // SAFETY: a question about a property.
    unsafe { AudioObjectHasProperty(id, &a) }
}

/// The sound as it is now.
pub fn read() -> HostAudio {
    let default: u32 = get(SYSTEM, addr(DEFAULT_OUT, GLOBAL)).unwrap_or(0);
    let mut out = HostAudio::default();
    for id in outputs() {
        let name = string(id, addr(UID, GLOBAL)).unwrap_or_else(|| id.to_string());
        if id == default {
            out.default_sink = name.clone();
        }
        out.sinks.push(Sink {
            id,
            name,
            label: string(id, addr(NAME, GLOBAL)).unwrap_or_default(),
            volume: get::<f32>(id, addr(VIRTUAL_VOLUME, OUTPUT)).unwrap_or(1.0),
            muted: get::<u32>(id, addr(MUTE, OUTPUT)).is_some_and(|m| m != 0),
        });
    }
    out
}

pub struct MacAudio {
    state: Published<HostAudio>,
}

impl MacAudio {
    pub fn new() -> MacAudio {
        MacAudio {
            state: Published::new(read()),
        }
    }
}

impl Default for MacAudio {
    fn default() -> Self {
        Self::new()
    }
}

impl AudioMixer for MacAudio {
    fn state(&self) -> watch::Receiver<HostAudio> {
        self.state.subscribe()
    }

    fn fresh(&self, within: Duration) -> Fut<'_, ()> {
        Box::pin(self.state.fresh(within, || async { read() }))
    }

    fn set_volume(&self, id: u32, volume: f32) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            if !volume.is_finite() {
                return Err("that is not a volume".into());
            }
            if !has(id, addr(VIRTUAL_VOLUME, OUTPUT)) {
                return Err("this output's volume is set on the device itself".into());
            }
            set(id, addr(VIRTUAL_VOLUME, OUTPUT), volume.clamp(0.0, 1.0))?;
            self.state.publish(read());
            Ok(())
        })
    }

    fn set_mute(&self, id: u32, muted: bool) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            set(id, addr(MUTE, OUTPUT), u32::from(muted))?;
            self.state.publish(read());
            Ok(())
        })
    }

    fn set_default(&self, name: String) -> Fut<'_, Res<()>> {
        Box::pin(async move {
            let now = read();
            let id = now
                .sinks
                .iter()
                .find(|s| s.name == name)
                .map(|s| s.id)
                .ok_or("that output is gone")?;
            set(SYSTEM, addr(DEFAULT_OUT, GLOBAL), id)?;
            self.state.publish(read());
            Ok(())
        })
    }

    fn move_stream(&self, _id: u32, _sink: String) -> Fut<'_, Res<()>> {
        Box::pin(async { Err(NO_STREAMS.to_string()) })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn volume_round_trip() {
        let a = read();
        let Some(dev) = a.sinks.iter().find(|s| s.name == a.default_sink).cloned() else {
            eprintln!("no output device here");
            return;
        };
        if !has(dev.id, addr(VIRTUAL_VOLUME, OUTPUT)) {
            eprintln!("{} has no volume", dev.label);
            return;
        }
        let m = MacAudio::new();
        m.set_volume(dev.id, dev.volume).await.unwrap();
        let back = read().sinks.into_iter().find(|s| s.id == dev.id).unwrap();
        assert!(
            (back.volume - dev.volume).abs() < 0.01,
            "{} → {}",
            dev.volume,
            back.volume
        );
    }

    #[tokio::test]
    async fn move_stream_is_not_on_macos() {
        let e = MacAudio::new()
            .move_stream(1, "x".into())
            .await
            .unwrap_err();
        assert!(e.starts_with("Not on macOS"));
    }

    #[test]
    fn outputs_have_labels() {
        for s in read().sinks {
            assert!(!s.name.is_empty());
        }
    }
}
