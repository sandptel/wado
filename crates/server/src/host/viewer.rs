//! "This phone" while a device is connected — the viewer sink's lifecycle, the policy over the
//! platform's [`ViewerSink`]: made for the device that connects, named after it, remade for a
//! different one with what was playing moved back onto it, removed when the last one goes.

use std::sync::Mutex;
use std::time::Duration;

use wado_platform::Platform;
use wado_platform::audio::SinkHandle;

/// The viewer's sink while one is connected, with the description it was made under.
#[derive(Default)]
pub struct Viewer {
    sink: Mutex<Option<(Box<dyn SinkHandle>, String)>>,
}

static VIEWER: Viewer = Viewer {
    sink: Mutex::new(None),
};

/// The viewer sink's name for this daemon run — fixed while the sink itself comes and goes —
/// or `None` when there is no viewer sink here (audio off).
pub fn name(p: &Platform) -> Option<String> {
    p.viewer_sink().ok().map(|s| s.name())
}

/// This daemon's viewer sink name (see [`name`]).
pub fn start() -> Option<String> {
    name(crate::platform::get())
}

/// The same as [`start`]: the output a viewer is shown as "This phone".
pub fn phone() -> Option<String> {
    start()
}

/// A device connected (`Some(name)`) or the last one went (`None`).
pub async fn viewer(device: Option<String>) {
    VIEWER.set(crate::platform::get(), device).await
}

impl Viewer {
    fn held(&self) -> std::sync::MutexGuard<'_, Option<(Box<dyn SinkHandle>, String)>> {
        self.sink.lock().unwrap_or_else(|e| e.into_inner())
    }

    /// Connected: the sink exists, described after the device. A different device than the sink
    /// was made for gets it remade under its own name — an output cannot be renamed — and the
    /// apps that were playing to it are moved back onto it. Gone: the sink is removed.
    pub async fn set(&self, p: &Platform, device: Option<String>) {
        let Ok(sinks) = p.viewer_sink() else { return };
        let name = sinks.name();
        let Some(device) = device else {
            if self.held().take().is_some() {
                tracing::info!(
                    sink = name,
                    "viewer audio sink removed — no device connected"
                );
            }
            return;
        };
        let description = format!(
            "wado · {}",
            if device.is_empty() {
                "viewer"
            } else {
                device.as_str()
            }
        );
        if self.held().as_ref().is_some_and(|(_, d)| *d == description) {
            return;
        }
        // Who plays to it now, to put back after the swap.
        let mixer = p.audio().ok();
        let playing: Vec<u32> = match &mixer {
            Some(m) => super::audio::now(m.as_ref(), Some(&name))
                .await
                .streams
                .iter()
                .filter(|s| s.sink.as_deref() == Some(name.as_str()))
                .map(|s| s.id)
                .collect(),
            None => Vec::new(),
        };
        self.held().take(); // the old one goes before the new one is made
        let sink = match sinks.open(description.clone()) {
            Ok(s) => s,
            Err(e) => {
                tracing::warn!("{e}");
                return;
            }
        };
        *self.held() = Some((sink, description));
        if let (Some(m), false) = (mixer, playing.is_empty()) {
            tokio::time::sleep(Duration::from_millis(300)).await;
            for id in playing {
                let _ = m.move_stream(id, name.clone()).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use wado_platform::fake::Fakes;
    use wado_protocol::host::{HostAudio, Stream};

    #[tokio::test(start_paused = true)]
    async fn the_sink_follows_the_device_and_keeps_what_played() {
        let f = Fakes::new();
        let p = f.platform();
        let v = Viewer::default();
        v.set(&p, Some("Pixel".into())).await;
        v.set(&p, Some("Pixel".into())).await; // the same device: nothing to do
        f.audio.state.will_read(HostAudio {
            streams: vec![Stream {
                id: 4,
                sink: Some("wado-fake".into()),
                ..Default::default()
            }],
            ..Default::default()
        });
        v.set(&p, Some("iPad".into())).await;
        v.set(&p, None).await;
        assert_eq!(
            f.log.calls(),
            [
                "sink.open wado · Pixel",
                "sink.close wado · Pixel",
                "sink.open wado · iPad",
                "audio.move_stream 4 wado-fake",
                "sink.close wado · iPad",
            ]
        );
    }
}
