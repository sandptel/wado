// SPDX-License-Identifier: AGPL-3.0-only
//! `wp_content_type_v1` — the client's own word on what it is drawing.
//!
//! Nothing in wado acts on this yet, and that is deliberate. The encoder's tuning is fixed at
//! session start (CBR, no B-frames, no lookahead — invariant 7), so there is no knob a per-
//! surface hint could turn today. What the hint *is* good for right now is telling us whether
//! the question is worth asking: "would the encoder benefit from knowing a video is playing?"
//! cannot be answered without first knowing whether any real app ever says so. So this logs
//! and stops, the same pattern `new_fractional_scale` uses.
//!
//! ⚠️ If the encoder ever learns to read the hint, it must not trust it for *quality* decisions
//! a client can game — the hint is client-supplied and unverified. The plausible use is the
//! opposite direction: `Video` on a full-screen surface means "expect sustained damage", which
//! is a pacing input, not a security-relevant one.

use std::collections::HashMap;

use smithay::reexports::wayland_protocols::wp::content_type::v1::server::wp_content_type_v1;
use smithay::reexports::wayland_server::Resource;
use smithay::reexports::wayland_server::backend::ObjectId;
use smithay::reexports::wayland_server::protocol::wl_surface::WlSurface;
use smithay::wayland::compositor::with_states;
use smithay::wayland::content_type::ContentTypeSurfaceCachedState;

/// Last content type logged per surface.
///
/// A surface commits at the frame rate, so logging every commit would bury the signal. Only a
/// *change* is news — including the change back to `None`, which is how an app says its video
/// stopped.
///
/// ponytail: entries are dropped wholesale on session stop rather than tracked per surface
/// destroy. A session's surface count is in the tens; per-surface cleanup buys nothing until
/// something reads the map at scale.
#[derive(Default)]
pub struct ContentTypeLog(HashMap<ObjectId, wp_content_type_v1::Type>);

impl ContentTypeLog {
    pub fn clear(&mut self) {
        self.0.clear();
    }

    /// Log this surface's content type if it differs from the last one seen.
    pub fn observe(&mut self, surface: &WlSurface) {
        let current = with_states(surface, |states| {
            *states
                .cached_state
                .get::<ContentTypeSurfaceCachedState>()
                .current()
                .content_type()
        });

        let id = surface.id();
        // `None` is the default for every surface that never bound the protocol, so an absent
        // entry and an explicit `None` mean the same thing and neither is worth a line.
        match self.0.get(&id) {
            Some(prev) if *prev == current => {}
            None if current == wp_content_type_v1::Type::None => {}
            _ => {
                tracing::info!(?current, "surface declared a content type");
                self.0.insert(id, current);
            }
        }
    }
}
