//! Rendering when some strip column is drawn smaller than its app's own size (S3).
//!
//! One job: build the output's element list by hand, so each window can be drawn at its own
//! factor. `space::render_output` draws every element at the output scale and offers no hook
//! for a per-element one, so while any column is shrunk the frame is composed here instead —
//! same elements, same order, same damage tracker; only the scale differs per window. With
//! nothing shrunk the render tick keeps using `space::render_output` untouched.
//!
//! The shrink is a `RescaleRenderElement` about the window's origin, not a smaller scale
//! handed to `render_elements`: a surface element sizes itself from the scale the damage
//! tracker passes at draw time (the output's), so a creation-time scale only moves its
//! subsurfaces. Measured with `examples/strip_check.rs` — the first version drew the app full
//! size with every factor correctly computed.
//!
//! Popups are included by `Window::render_elements`, so an app's menus shrink with it and stay
//! attached where the app placed them.

use smithay::backend::renderer::element::AsRenderElements;
use smithay::backend::renderer::element::solid::SolidColorRenderElement;
use smithay::backend::renderer::element::surface::WaylandSurfaceRenderElement;
use smithay::backend::renderer::element::utils::RescaleRenderElement;
use smithay::backend::renderer::gles::GlesRenderer;
use smithay::desktop::Window;
use smithay::utils::{Logical, Point, Rectangle, Scale};

smithay::backend::renderer::element::render_elements! {
    pub ScaledElement<=GlesRenderer>;
    Surface=WaylandSurfaceRenderElement<GlesRenderer>,
    Shrunk=RescaleRenderElement<WaylandSurfaceRenderElement<GlesRenderer>>,
    Solid=SolidColorRenderElement,
}

/// One window to draw: its geometry origin (output-relative, logical) — the point it shrinks
/// towards — and the factor.
pub struct Placed {
    pub window: Window,
    pub origin: Point<i32, Logical>,
    pub factor: f64,
}

/// The windows on `output_geo`, front first, with their factors. Taken from the state before
/// the renderer is borrowed; see the render tick.
pub fn placed(state: &crate::Wado, output_geo: Rectangle<i32, Logical>) -> Vec<Placed> {
    state
        .space
        .elements()
        .rev()
        .filter_map(|w| {
            let loc = state.space.element_location(w)?;
            let f = state.window_scale(w);
            let geo = w.geometry();
            let visual = Rectangle::new(loc, geo.size.to_f64().upscale(f).to_i32_ceil());
            if !visual.overlaps(output_geo) {
                return None;
            }
            Some(Placed {
                window: w.clone(),
                origin: loc - output_geo.loc,
                factor: f,
            })
        })
        .collect()
}

/// The element list for one frame: overlay elements on top, then every window at its factor.
pub fn elements(
    renderer: &mut GlesRenderer,
    placed: &[Placed],
    overlay: &[SolidColorRenderElement],
    output_scale: f64,
) -> Vec<ScaledElement> {
    let mut out: Vec<ScaledElement> = overlay.iter().cloned().map(Into::into).collect();
    for p in placed {
        // Drawn exactly where the space would draw it — the buffer origin sits `geometry.loc`
        // before the geometry (CSD shadows live there) — then shrunk about the geometry origin,
        // which takes that margin down with it.
        let origin = p.origin.to_physical_precise_round(output_scale);
        let at = (p.origin - p.window.geometry().loc).to_physical_precise_round(output_scale);
        let surfaces = p
            .window
            .render_elements::<WaylandSurfaceRenderElement<GlesRenderer>>(
                renderer,
                at,
                Scale::from(output_scale),
                1.0,
            );
        if p.factor < 1.0 {
            out.extend(
                surfaces
                    .into_iter()
                    .map(|e| RescaleRenderElement::from_element(e, origin, p.factor).into()),
            );
        } else {
            out.extend(surfaces.into_iter().map(Into::into));
        }
    }
    out
}
