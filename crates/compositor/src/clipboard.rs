//! The clipboard, both ways, as text.
//!
//! **Phone → session:** the viewer sends text; the compositor takes the Wayland selection with
//! it, and an app that pastes is served from [`SelectionHandler::send_selection`].
//! **Session → phone:** an app copies; the compositor reads the selection through a pipe and
//! publishes it on a watch channel the server forwards to the viewer.
//!
//! Text only, capped at [`MAX`]. Images and files are the upgrade path, and the reason the
//! cap exists: an app offering a 40 MB image as `text/uri-list` must not stall anything.

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

/// What a text selection is offered as, best first.
const TEXT: &[&str] = &[
    "text/plain;charset=utf-8",
    "text/plain",
    "UTF8_STRING",
    "TEXT",
    "STRING",
];

impl Wado {
    /// Make `text` the session's clipboard.
    pub(crate) fn set_clipboard(&mut self, text: String) {
        let text: Arc<str> = text.chars().take(MAX).collect::<String>().into();
        set_data_device_selection(
            &self.display_handle,
            &self.seat,
            TEXT.iter().map(|s| s.to_string()).collect(),
            text,
        );
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
        let Some(mime) = TEXT.iter().find(|m| offered.iter().any(|o| o == *m)) else {
            return; // not text — nothing to hand a phone's clipboard
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
        if let Err(e) = request_data_device_client_selection(&self.seat, mime, writer.into()) {
            tracing::warn!("could not read the session clipboard: {e:?}");
            return;
        }
        let tx = self.clipboard_tx.clone();
        std::thread::spawn(move || {
            let mut buf = Vec::new();
            // The writer end is the app's now; EOF arrives when it has written and closed.
            if (&mut reader).take(MAX as u64).read_to_end(&mut buf).is_ok() && !buf.is_empty() {
                let _ = tx.send(String::from_utf8_lossy(&buf).into_owned());
            }
        });
    }
}

/// An app pasting the selection the viewer gave us: write it, off the loop thread — a pipe the
/// app is slow to drain must not stall rendering.
pub(crate) fn serve(fd: std::os::fd::OwnedFd, text: &Arc<str>) {
    let text = Arc::clone(text);
    std::thread::spawn(move || {
        let _ = std::fs::File::from(fd).write_all(text.as_bytes());
    });
}
