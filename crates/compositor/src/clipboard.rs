//! The clipboard, both ways, as text or an image.
//!
//! **Into the session** (the viewer, or the host desktop's clipboard): the compositor takes the
//! Wayland selection with the bytes, and an app that pastes is served from
//! [`SelectionHandler::send_selection`].
//! **Out of the session:** an app copies; the compositor reads the selection through a pipe and
//! publishes it on a watch channel the server forwards.
//!
//! Capped at [`MAX`] (text) and [`MAX_IMAGE`]: an app offering something huge must not stall
//! anything.
//!
//! [`SelectionHandler::send_selection`]: smithay::wayland::selection::SelectionHandler

use std::{
    io::{Read, Write},
    sync::Arc,
};

use smithay::wayland::selection::{
    SelectionSource, SelectionTarget,
    data_device::{request_data_device_client_selection, set_data_device_selection},
};

use crate::Wado;

pub const MAX: usize = 1 << 20;
pub const MAX_IMAGE: usize = 16 << 20;

/// One clipboard value. Empty `data` is nothing copied yet.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Clip {
    pub mime: String,
    pub data: Arc<[u8]>,
}

impl Clip {
    pub fn is_text(&self) -> bool {
        self.mime.starts_with("text/") || TEXT.contains(&self.mime.as_str())
    }
}

/// What a text selection is offered as, best first.
const TEXT: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "TEXT",
    "STRING",
];

/// Images read out of the session, best first.
const IMAGE: &[&str] = &["image/png", "image/jpeg", "image/webp", "image/gif"];

impl Wado {
    /// Make `clip` the session's clipboard.
    pub(crate) fn set_clipboard(&mut self, clip: Clip) {
        let (offer, cap) = if clip.is_text() {
            (TEXT.iter().map(|s| s.to_string()).collect(), MAX)
        } else {
            (vec![clip.mime.clone()], MAX_IMAGE)
        };
        if clip.data.is_empty() || clip.data.len() > cap {
            return;
        }
        set_data_device_selection(&self.display_handle, &self.seat, offer, clip.data);
    }

    /// An app set the clipboard: read it, off the loop thread, and publish it.
    pub(crate) fn clipboard_changed(
        &mut self,
        ty: SelectionTarget,
        source: Option<SelectionSource>,
    ) {
        if ty != SelectionTarget::Clipboard {
            return;
        }
        let Some(source) = source else { return };
        let offered = source.mime_types();
        tracing::debug!(?offered, "session clipboard changed");
        let Some(mime) = TEXT
            .iter()
            .chain(IMAGE)
            .find(|m| offered.iter().any(|o| o == *m))
        else {
            return; // neither text nor an image we pass on
        };
        // On the next idle, not now: Smithay calls this *before* it records the new selection on
        // the seat, so a read from inside the callback finds `NoSelection`.
        let mime = mime.to_string();
        self.loop_handle
            .insert_idle(move |state| state.read_clipboard(mime));
    }

    fn read_clipboard(&mut self, mime: String) {
        let Ok((mut reader, writer)) = std::io::pipe() else {
            return;
        };
        if let Err(e) =
            request_data_device_client_selection(&self.seat, mime.clone(), writer.into())
        {
            tracing::warn!("could not read the session clipboard: {e:?}");
            return;
        }
        let tx = self.clipboard_tx.clone();
        let cap = if TEXT.contains(&mime.as_str()) {
            MAX
        } else {
            MAX_IMAGE
        };
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            // The writer end is the app's now; EOF arrives when it has written and closed.
            if (&mut reader).take(cap as u64).read_to_end(&mut buf).is_ok() && !buf.is_empty() {
                let _ = tx.send(Clip {
                    mime,
                    data: buf.into(),
                });
            }
        });
    }
}

/// An app pasting the selection we set: write it, off the loop thread — a pipe the app is slow
/// to drain must not stall rendering.
pub(crate) fn serve(fd: std::os::fd::OwnedFd, data: &Arc<[u8]>) {
    let data = Arc::clone(data);
    std::thread::spawn(move || {
        let _ = std::fs::File::from(fd).write_all(&data);
    });
}
