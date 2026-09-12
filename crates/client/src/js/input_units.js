// wado bridge — the one conversion between the viewer's pixels and the session's.
//
// Its own file because two callers need it — the wheel in input_pointer.js and the two-finger
// drag in input_scroll.js — and because getting it wrong is invisible from the outside: the
// scroll still works, it just asks for the wrong amount of finger.
//
// THE BUG THIS CLOSES. A pointer event's clientX/clientY are **CSS pixels of the viewer's
// viewport**. A `wl_pointer.axis` value is **surface-local logical pixels of the compositor**.
// Nothing converted between them, so every delta was measured at the viewer's scale and spent
// at the session's. On a 1080-wide session displayed in a ~393 CSS-px-wide video that is a
// 2.75x shortfall, and the output's own fractional scale (1.75) gives 1.57x of it back — so the
// finger had to travel about 1.6x further than the content it was dragging. Worse, the
// acceleration curve in input_accel.js still applied, so a slow drag under-scrolled and a fast
// flick over-shot: the same gesture was wrong in two directions depending on speed.
//
// Two divisions, and both are load-bearing:
//
//   fit          CSS px per video pixel under `object-fit: contain` — the same factor
//                `normPoint` uses to place a touch. Undoes the video being shrunk onto the
//                phone's screen.
//   outputScale  the session's fractional scale. Undoes the compositor's own logical-to-
//                physical ratio, because the axis is spoken in logical pixels and the video
//                carries physical ones.
//
// The property this buys is the whole point: **a finger moving N pixels across the glass
// scrolls the content N pixels across the glass.** That is what a touchscreen means by
// scrolling, and it is why the right answer here is a conversion rather than a tuned constant —
// a constant would have to be re-tuned for every resolution, scale and screen size.

// The running session's fractional scale. Set from the SessionConfig the Rust UI already hands
// to `W.start`, so this needs no plumbing of its own. Defaults to 1 rather than 0: it is a
// divisor.
W.outputScale = 1;

/// Multiplier taking a CSS-pixel delta to a compositor logical-pixel delta.
W.cssToLogical = (video) => {
  if (!video) return 1;
  const r = video.getBoundingClientRect();
  const vw = video.videoWidth, vh = video.videoHeight;
  // Before the first frame decodes there is no video size and therefore no honest factor.
  // Returning 1 keeps the previous behaviour for that window instead of inventing a number.
  if (!vw || !vh || !r.width || !r.height) return 1;
  const fit = Math.min(r.width / vw, r.height / vh);
  if (!(fit > 0)) return 1;
  const s = W.outputScale > 0 ? W.outputScale : 1;
  return 1 / (fit * s);
};
