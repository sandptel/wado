//! A glow around the focused window.
//!
//! **Why the compositor draws this and not the client.** Focus here is not what a desktop's
//! focus is. The bar's window actions — maximize, close, next window — all act on "the focused
//! window", and with focus-follows-pointer on it changes under the pointer with nothing to show
//! for it. A client-drawn title bar cannot help: an application with server-side decorations has
//! no title bar to draw, an X11 client under the session's Xwayland has no title bar *and* no
//! window manager, and a single fullscreen application has nothing visible either way. The only
//! place that knows which window the next ✕ will close is here.
//!
//! **How it is drawn.** Four solid-colour rectangles per ring, tiled around the window's
//! geometry so they never overlap it or each other, with a second wider ring at low alpha to
//! fake the falloff. No shader, no blur, no texture: a real gaussian glow means a second render
//! pass and a framebuffer, and at streaming resolutions two rings are indistinguishable from one
//! after H.264 has had its say.
//!
//! **The buffers live on the state, and that is the whole performance story.** A
//! [`SolidColorBuffer`] carries a stable id and a commit counter that only moves when its size
//! or colour does, so a focused window sitting still produces *no damage* and the frame stays
//! empty. Building fresh elements each tick would give every frame a new element id, which the
//! damage tracker can only read as "everything changed" — a full-screen damage rectangle every
//! frame, at a fixed bitrate, forever.
//!
//! ponytail: no per-window colour, no animation, no setting. A ring is on the focused window or
//! there is no ring.

use smithay::{
    backend::renderer::element::{
        Kind,
        solid::{SolidColorBuffer, SolidColorRenderElement},
    },
    utils::{Logical, Point, Rectangle},
};

/// One ring: how far outside the window it starts, how thick it is, and how solid.
struct Ring {
    inset: i32,
    thickness: i32,
    alpha: f32,
}

/// Inner ring reads as the border; outer ring at low alpha reads as the glow.
///
/// Logical pixels, so the ring keeps its apparent thickness when the session scale changes —
/// the same reason the rest of the layout is in logical coordinates.
const RINGS: &[Ring] = &[
    Ring {
        inset: 0,
        thickness: 2,
        alpha: 1.0,
    },
    Ring {
        inset: 2,
        thickness: 5,
        alpha: 0.28,
    },
];

/// The accent. Fixed rather than themed: the client's base16 scheme is a browser-side thing and
/// plumbing it down to the compositor would mean a session setting, a protocol field and a
/// reconfigure path for a colour.
const COLOR: [f32; 4] = [0.29, 0.62, 1.0, 1.0];

/// The ring buffers, one set for the life of the session.
#[derive(Debug)]
pub struct Glow {
    buffers: Vec<SolidColorBuffer>,
}

impl Default for Glow {
    fn default() -> Self {
        Self::new()
    }
}

impl Glow {
    pub fn new() -> Self {
        Self {
            buffers: (0..RINGS.len() * 4)
                .map(|_| SolidColorBuffer::new((0, 0), COLOR))
                .collect(),
        }
    }

    /// The elements to draw for a focused window at `geo`, in coordinates relative to the
    /// output's own origin. Empty when nothing is focused.
    ///
    /// `scale` is the output scale: the rectangles are computed in logical pixels and handed to
    /// smithay in physical ones, which is what keeps a 2px ring 2px-looking at scale 2.
    pub fn elements(
        &mut self,
        geo: Option<Rectangle<i32, Logical>>,
        scale: f64,
    ) -> Vec<SolidColorRenderElement> {
        let Some(geo) = geo.filter(|g| !g.size.is_empty()) else {
            return Vec::new();
        };
        let mut out = Vec::with_capacity(self.buffers.len());
        for (ring, rects) in RINGS.iter().zip(self.buffers.chunks_mut(4)) {
            for (buffer, rect) in rects
                .iter_mut()
                .zip(ring_rects(geo, ring.inset, ring.thickness))
            {
                // `update` is a no-op when nothing moved, which is what keeps a still window
                // from damaging the output every tick.
                buffer.update(rect.size, COLOR);
                out.push(SolidColorRenderElement::from_buffer(
                    buffer,
                    rect.loc.to_physical_precise_round(scale),
                    scale,
                    ring.alpha,
                    Kind::Unspecified,
                ));
            }
        }
        out
    }
}

/// The four rectangles of one ring around `geo`: top, bottom, left, right.
///
/// Tiled rather than drawn as an outlined box, because these are filled quads — an expanded
/// filled rectangle would cover the window, and custom elements render *in front of* the space,
/// so there is no "behind" to hide it in. The top and bottom bars carry the corners so the left
/// and right ones can stop at the window's own height; nothing overlaps, which is what lets the
/// low-alpha ring look even instead of blotchy where two translucent quads meet.
fn ring_rects(
    geo: Rectangle<i32, Logical>,
    inset: i32,
    thickness: i32,
) -> [Rectangle<i32, Logical>; 4] {
    let (x, y, w, h) = (geo.loc.x, geo.loc.y, geo.size.w, geo.size.h);
    let out = inset + thickness;
    let rect = |x: i32, y: i32, w: i32, h: i32| {
        Rectangle::new(Point::from((x, y)), (w.max(0), h.max(0)).into())
    };
    [
        rect(x - out, y - out, w + 2 * out, thickness),
        rect(x - out, y + h + inset, w + 2 * out, thickness),
        rect(x - out, y - inset, thickness, h + 2 * inset),
        rect(x + w + inset, y - inset, thickness, h + 2 * inset),
    ]
}

#[cfg(test)]
mod tests {
    use super::*;

    fn window() -> Rectangle<i32, Logical> {
        Rectangle::new(Point::from((100, 50)), (400, 300).into())
    }

    /// The property that matters for a translucent ring: no quad may cover another, or the
    /// overlap shows up as a brighter seam.
    fn overlaps(a: Rectangle<i32, Logical>, b: Rectangle<i32, Logical>) -> bool {
        a.intersection(b).is_some_and(|i| !i.size.is_empty())
    }

    #[test]
    fn a_ring_encloses_the_window_without_touching_it() {
        let g = window();
        let rects = ring_rects(g, 0, 2);
        for r in rects {
            assert!(!overlaps(r, g), "a ring quad covered the window: {r:?}");
        }
        // Top and bottom span the full width including the corners; left and right stop at the
        // window's own height. Together that is a closed ring.
        assert_eq!(rects[0].size.w, g.size.w + 4);
        assert_eq!(rects[2].size.h, g.size.h);
        assert_eq!(rects[0].loc.y, g.loc.y - 2);
        assert_eq!(rects[1].loc.y, g.loc.y + g.size.h);
    }

    #[test]
    fn quads_never_overlap_each_other() {
        let g = window();
        let all: Vec<_> = RINGS
            .iter()
            .flat_map(|r| ring_rects(g, r.inset, r.thickness))
            .collect();
        for (i, a) in all.iter().enumerate() {
            for b in &all[i + 1..] {
                assert!(!overlaps(*a, *b), "two ring quads overlap: {a:?} {b:?}");
            }
        }
    }

    #[test]
    fn the_outer_ring_sits_outside_the_inner_one() {
        let g = window();
        let inner = ring_rects(g, RINGS[0].inset, RINGS[0].thickness);
        let outer = ring_rects(g, RINGS[1].inset, RINGS[1].thickness);
        // Top edges: the outer ring's top must start above the inner ring's.
        assert!(outer[0].loc.y < inner[0].loc.y);
        assert_eq!(outer[0].loc.y + outer[0].size.h, inner[0].loc.y);
    }

    #[test]
    fn nothing_is_drawn_without_a_focused_window() {
        let mut glow = Glow::new();
        assert!(glow.elements(None, 1.0).is_empty());
        // A window that has not committed a size yet is not a window to ring.
        let empty = Rectangle::new(Point::from((0, 0)), (0, 0).into());
        assert!(glow.elements(Some(empty), 1.0).is_empty());
        assert_eq!(glow.elements(Some(window()), 1.0).len(), RINGS.len() * 4);
    }
}
