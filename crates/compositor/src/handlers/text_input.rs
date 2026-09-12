// SPDX-License-Identifier: AGPL-3.0-only
//! `zwp_text_input_v3` — but only to learn **when an app wants typing**.
//!
//! A phone has no keys, so the soft keyboard has to be raised by something. Until now that
//! something was the ⌨ button: correct, and one tap too many every time you touch a text field.
//! This protocol is how an application says "a text field just took focus", which is exactly
//! the signal that tap was standing in for.
//!
//! ## Why this is hand-rolled rather than smithay's `TextInputManagerState`
//!
//! Smithay's implementation discards every text-input request unless some client has bound
//! `zwp_input_method_v2` (`text_input_handle.rs:209`, `has_instance()`), and an instance can
//! only be created through the input-method manager's dispatch — there is no server-side way to
//! register one. wado would therefore have to *be* a second Wayland client of its own display,
//! with a `wayland-client` dependency, a socketpair and a second event loop, to obtain one bit
//! of state. That is a great deal of machinery for a boolean.
//!
//! So wado implements the protocol itself, as a **pure observer**, and does not register
//! smithay's manager. Two globals of the same interface would both be advertised and clients
//! would bind whichever they saw first.
//!
//! ## It is deliberately an empty IME, and that is a supported configuration
//!
//! wado accepts `enable`/`disable`/`commit`, sends `enter`/`leave`/`done`, and **never** sends
//! `preedit_string` or `commit_string`. That is precisely the situation on Sway and Hyprland
//! with no input method running: the protocol is advertised, nothing drives it, and typing works
//! because toolkits still take key events from `wl_keyboard` — which is where wado's synthesized
//! keys arrive. Do not extend this into a real IME without deciding what would drive it.
//!
//! ## Two details the protocol punishes
//!
//! * **`enable` is double-buffered.** It takes effect on `commit`, not when the request arrives.
//!   Signalling on `enable` reports fields that are then never committed.
//! * **`leave` must be sent when focus goes away**, or the client keeps an enabled surface it no
//!   longer owns and the keyboard stays up over the wrong window.

use std::sync::{Arc, Mutex};

use smithay::reexports::wayland_protocols::wp::text_input::zv3::server::{
    zwp_text_input_manager_v3::{self, ZwpTextInputManagerV3},
    zwp_text_input_v3::{self, ZwpTextInputV3},
};
use smithay::reexports::wayland_server::{
    protocol::wl_surface::WlSurface, Client, DataInit, Dispatch, DisplayHandle, GlobalDispatch,
    New, Resource,
};

use crate::Wado;

/// Per-`zwp_text_input_v3` object state.
///
/// `pending` is what `enable`/`disable` set; `enabled` is what the last `commit` made real.
#[derive(Default)]
pub struct TextInputData {
    inner: Arc<Mutex<Inner>>,
}

#[derive(Default)]
struct Inner {
    pending: bool,
    enabled: bool,
    serial: u32,
}

/// Every live text-input object, with the surface each belongs to once it has been entered.
///
/// ponytail: a `Vec` scanned linearly. A session has a handful of these; a map buys nothing
/// until something here runs per-frame, and nothing does.
#[derive(Default)]
pub struct TextInputs {
    objects: Vec<(ZwpTextInputV3, Arc<Mutex<Inner>>)>,
    /// The surface currently holding keyboard focus, so a newly-created object can be entered
    /// immediately rather than waiting for the next focus change.
    focus: Option<WlSurface>,
    /// Whether any focused object is committed-enabled — the one bit this module exists for.
    active: bool,
}

impl TextInputs {
    /// Keyboard focus moved. Sends `leave` to the old surface's objects and `enter` to the
    /// new one's, which is what tells a toolkit it may start using the protocol.
    pub fn focus_changed(&mut self, surface: Option<&WlSurface>) {
        if self.focus.as_ref().map(|s| s.id()) == surface.map(|s| s.id()) {
            return;
        }
        if let Some(old) = self.focus.take() {
            for (obj, state) in &self.objects {
                if obj.id().same_client_as(&old.id()) {
                    obj.leave(&old);
                    // Leaving resets the client's state per the protocol; ours must follow or a
                    // surface re-entered later looks enabled when the client thinks it is not.
                    let mut s = state.lock().unwrap();
                    s.pending = false;
                    s.enabled = false;
                }
            }
        }
        if let Some(new) = surface {
            for (obj, _) in &self.objects {
                if obj.id().same_client_as(&new.id()) {
                    obj.enter(new);
                }
            }
            self.focus = Some(new.clone());
        }
        self.recompute();
    }

    /// Whether the focused application is asking for text input right now.
    pub fn active(&self) -> bool {
        self.active
    }

    fn recompute(&mut self) {
        let focus = self.focus.as_ref();
        self.active = focus.is_some_and(|f| {
            self.objects.iter().any(|(obj, state)| {
                obj.id().same_client_as(&f.id()) && state.lock().unwrap().enabled
            })
        });
    }

    fn retain_live(&mut self) {
        self.objects.retain(|(obj, _)| obj.is_alive());
    }
}

impl GlobalDispatch<ZwpTextInputManagerV3, ()> for Wado {
    fn bind(
        _state: &mut Self,
        _handle: &DisplayHandle,
        _client: &Client,
        resource: New<ZwpTextInputManagerV3>,
        _global_data: &(),
        data_init: &mut DataInit<'_, Self>,
    ) {
        // Logged because the alternative is guessing. "The keyboard did not come up" has two
        // very different causes — the app never asked for text input, or it asked and the
        // client did not respond — and only this line separates them. Chromium, notably, binds
        // this global only when started with `--enable-wayland-ime`; without it no amount of
        // tapping a text field produces a request, and nothing else in the log would say so.
        tracing::info!("a client bound zwp_text_input_manager_v3");
        data_init.init(resource, ());
    }
}

impl Dispatch<ZwpTextInputManagerV3, ()> for Wado {
    fn request(
        state: &mut Self,
        _client: &Client,
        _resource: &ZwpTextInputManagerV3,
        request: zwp_text_input_manager_v3::Request,
        _data: &(),
        _dh: &DisplayHandle,
        data_init: &mut DataInit<'_, Self>,
    ) {
        match request {
            zwp_text_input_manager_v3::Request::GetTextInput { id, seat: _ } => {
                let data = TextInputData::default();
                let inner = data.inner.clone();
                let obj = data_init.init(id, data);
                // A client that binds while already focused must be told so now; there will be
                // no further focus change to carry the `enter`.
                if let Some(f) = state.text_inputs.focus.clone() {
                    if obj.id().same_client_as(&f.id()) {
                        obj.enter(&f);
                    }
                }
                state.text_inputs.retain_live();
                state.text_inputs.objects.push((obj, inner));
            }
            zwp_text_input_manager_v3::Request::Destroy => {}
            _ => {}
        }
    }
}

impl Dispatch<ZwpTextInputV3, TextInputData> for Wado {
    fn request(
        state: &mut Self,
        _client: &Client,
        resource: &ZwpTextInputV3,
        request: zwp_text_input_v3::Request,
        data: &TextInputData,
        _dh: &DisplayHandle,
        _data_init: &mut DataInit<'_, Self>,
    ) {
        {
            let mut inner = data.inner.lock().unwrap();
            match request {
                // Double-buffered: these only mark intent. `commit` is what applies it.
                zwp_text_input_v3::Request::Enable => inner.pending = true,
                zwp_text_input_v3::Request::Disable => inner.pending = false,
                zwp_text_input_v3::Request::Commit => {
                    inner.enabled = inner.pending;
                    // The serial must advance on every commit whether or not anything is driving
                    // the protocol, or a client that does track it desyncs.
                    inner.serial = inner.serial.wrapping_add(1);
                    resource.done(inner.serial);
                }
                // Accepted and ignored: cursor rectangle, surrounding text and content type are
                // an IME's inputs, and wado is not one. Dropping them is not a protocol error.
                _ => {}
            }
        }
        state.text_inputs.retain_live();
        state.text_inputs.recompute();
    }

    fn destroyed(
        state: &mut Self,
        _client: smithay::reexports::wayland_server::backend::ClientId,
        _resource: &ZwpTextInputV3,
        _data: &TextInputData,
    ) {
        state.text_inputs.retain_live();
        state.text_inputs.recompute();
    }
}
