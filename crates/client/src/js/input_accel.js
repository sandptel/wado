// wado bridge — pointer acceleration for scrolling.
//
// Its own file because it is one job with one exported function, shared by the two scroll
// paths that would otherwise each grow their own copy: the wheel (input_pointer.js) and the
// two-finger drag (input_scroll.js).
//
// Why acceleration rather than just a smaller multiplier. A single linear factor cannot be
// right twice: set it slow enough to place a line precisely and a long page takes a dozen
// swipes; set it fast enough to cross the page and small adjustments overshoot. Every
// touchpad driver solves this the same way — gain rises with the speed of the movement, so
// a slow drag is precise and a flick still travels. That is what "touchpad friendly" means
// here, and it is why lowering the speed slider alone was not the fix.

// Velocity, in CSS pixels per millisecond, at which gain reaches its maximum. About what a
// relaxed swipe produces; below it gain falls off towards 1.
const REF_VELOCITY = 1.2;

// Ceiling on the gain. Deliberately modest: this multiplies a base speed the user chose, and
// a curve that can quadruple it makes the slider feel like it stopped working.
const MAX_GAIN = 2.0;

// Floor on the inter-event gap. Browsers can deliver several deltas with the same or near
// timestamp — a coalesced burst, or a frame that ran long — and dividing by that would
// report an enormous velocity and slam the gain to maximum on a movement that was slow.
const MIN_DT_MS = 4;

// Gap beyond which the previous event is not part of this gesture. Without it, the first
// delta after a pause inherits a huge dt, reads as near-zero velocity, and the gesture
// starts unaccelerated regardless of how fast it actually began.
const GESTURE_GAP_MS = 200;

W.scrollAccel = {
  _last: 0,

  /// Gain for a movement of `magnitude` CSS pixels, from its speed since the last call.
  /// Returns a number in [1, MAX_GAIN]; callers multiply their delta by it.
  gain(magnitude) {
    const now = performance.now();
    const gap = now - this._last;
    this._last = now;
    // First event of a gesture: no velocity to measure yet, so do not guess one.
    if (!(gap > 0) || gap > GESTURE_GAP_MS) return 1;
    const v = Math.abs(magnitude) / Math.max(gap, MIN_DT_MS);
    return 1 + (MAX_GAIN - 1) * Math.min(v / REF_VELOCITY, 1);
  },
};
