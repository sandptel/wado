// wado bridge — two-finger scroll.
//
// Its own file because input_touch.js owns exactly one job: the per-primary-contact FSM.
// This owns the other one: what a *second* simultaneous contact means.
//
// Why it exists at all. Until now the only scroll path was the mouse wheel, so on a phone
// a drag became a wl_touch contact and scrolling worked only in applications that implement
// touch scrolling themselves. Most desktop toolkits expect a scroll axis, so the common case
// was a drag that selected text instead of scrolling the page.
//
// Shape: a second contact converts the gesture into a scroll, retracting whatever the first
// finger had already started — the primary may already have sent a touch-down, and leaving
// that live would drag a selection underneath the scroll. `cancel_touch` is the same retract
// the press-hold path uses.

// Below this the two contacts are treated as noise rather than intent; a resting hand moves
// a pixel or two per frame and should not scroll the page.
const SCROLL_DEADZONE = 2;

// The same gesture also carries a pinch: the midpoint moving is a scroll, the gap between
// the contacts changing is a magnify. Both are reported, which is what libinput does for a
// touchpad and what toolkits are written against — one drag can legitimately pan and zoom.
//
// Its own deadzones, and they are not the scroll one. A pure pinch holds the midpoint still,
// so it never clears SCROLL_DEADZONE; a pure scroll holds the gap constant, so it never
// clears these. That is also why the pinch is computed *before* the scroll deadzone returns.
const PINCH_DEADZONE = 0.01;    // fraction of the starting gap
const ROTATE_DEADZONE = 0.5;    // degrees

W.scrollg = {
  // Take over from the primary-contact FSM. Called on the second pointerdown.
  begin(e, video) {
    const g = W.gesture;
    if (!g) return false;
    // A third contact must not restart the gesture. Without this it re-ran the whole begin:
    // the scroll anchors jumped to the new pair, and the pinch baseline reset mid-zoom.
    if (g.state === "scroll") return true;
    if (g.holdTimer) { clearTimeout(g.holdTimer); g.holdTimer = null; } // else: right-click
    // Retract anything the first finger already committed to.
    if (g.state === "tap" || g.state === "touch") {
      W.coalesce.now({ t: "cancel_touch", id: g.id >>> 0 });
    } else if (g.state === "move") {
      // A window drag in flight: end it where it stands rather than leaving it grabbed.
      const n = W.normPoint(e.clientX, e.clientY, video);
      if (n) W.coalesce.now({ t: "window_drag", phase: "up", x: n.x, y: n.y });
    }
    g.state = "scroll";
    g.second = e.pointerId;
    // Midpoint of the two contacts, so rotating or pinching the pair does not scroll.
    g.sx = (g.lastClientX ?? g.startClientX) ;
    g.sy = (g.lastClientY ?? g.startClientY);
    g.anchorX = (g.sx + e.clientX) / 2;
    g.anchorY = (g.sy + e.clientY) / 2;
    g.p1 = { x: g.sx, y: g.sy };
    g.p2 = { x: e.clientX, y: e.clientY };
    // Pinch baseline. The gap can be zero if both contacts land on the same pixel, and a
    // zero baseline makes every later scale Infinity, so it is floored at one pixel.
    g.pinchD0 = Math.max(1, Math.hypot(g.p2.x - g.p1.x, g.p2.y - g.p1.y));
    g.pinchScale = 1;
    g.pinchAngle = Math.atan2(g.p2.y - g.p1.y, g.p2.x - g.p1.x);
    const pn = W.normPoint(g.anchorX, g.anchorY, video);
    if (pn) {
      g.pinchOn = true;
      // Its own last-point, not the scroll's: a pure pinch never fires a scroll, so
      // reusing g.lastN would leave the end event with nowhere to land and never send it.
      g.pinchN = pn;
      W.sendInput({ t: "pinch", phase: "down", x: pn.x, y: pn.y, scale: 1, rotation: 0 });
    }
    return true;
  },

  // A move from either contact. Returns true when it was consumed as scrolling.
  move(e, video) {
    const g = W.gesture;
    if (!g || g.state !== "scroll") return false;
    if (e.pointerId === g.id) g.p1 = { x: e.clientX, y: e.clientY };
    else if (e.pointerId === g.second) g.p2 = { x: e.clientX, y: e.clientY };
    else return false;

    const mx = (g.p1.x + g.p2.x) / 2;
    const my = (g.p1.y + g.p2.y) / 2;

    // Pinch first: a magnify with a still midpoint would otherwise be swallowed by the
    // scroll deadzone below and never reach the app at all.
    if (g.pinchOn) {
      const gap = Math.hypot(g.p2.x - g.p1.x, g.p2.y - g.p1.y);
      const scale = gap / g.pinchD0;
      const angle = Math.atan2(g.p2.y - g.p1.y, g.p2.x - g.p1.x);
      // Wrapped into (-180, 180]: without it, a gesture crossing the atan2 branch cut
      // reports a 360-degree flick in one event.
      let rot = ((angle - g.pinchAngle) * 180) / Math.PI;
      rot -= 360 * Math.round(rot / 360);
      if (Math.abs(scale - g.pinchScale) > PINCH_DEADZONE || Math.abs(rot) > ROTATE_DEADZONE) {
        const pn = W.normPoint(mx, my, video);
        if (pn) {
          // scale is absolute against the gap at "down"; rotation is the delta since the
          // last event. The protocol defines them that way — see InputEvent::Pinch.
          W.sendInput({ t: "pinch", phase: "motion", x: pn.x, y: pn.y, scale, rotation: rot });
          g.pinchScale = scale;
          g.pinchAngle = angle;
          g.pinchN = pn;
        }
      }
    }

    const dx = mx - g.anchorX;
    const dy = my - g.anchorY;
    if (Math.hypot(dx, dy) < SCROLL_DEADZONE) return true;
    g.anchorX = mx;
    g.anchorY = my;

    const n = W.normPoint(mx, my, video);
    if (!n) return true;
    // Content follows the finger, which is what a touchscreen means by scrolling — so the
    // delta is negated before the shared direction/speed settings are applied. Routed through
    // the same W.naturalScroll and W.scrollSpeed as the wheel so one setting governs both.
    const sign = W.naturalScroll ? -1 : 1;
    // Gain from the speed of the drag, so a slow adjustment stays precise and a flick still
    // crosses the page — see input_accel.js. Taken once from the combined magnitude rather
    // than per axis, or a diagonal drag would accelerate its two axes by different amounts
    // and curve away from the finger.
    // `cssToLogical` is what makes the content keep up with the finger: dx is in the viewer's
    // CSS pixels and the axis is consumed in the session's logical ones. See input_units.js.
    const speed =
      (W.scrollSpeed || 1) * W.scrollAccel.gain(Math.hypot(dx, dy)) * W.cssToLogical(video);
    W.sendInput({
      t: "scroll",
      x: n.x,
      y: n.y,
      dx: -dx * speed * sign,
      dy: -dy * speed * sign,
      source: "finger",
    });
    g.lastN = n;
    return true;
  },

  // Either contact lifting ends the gesture; a leftover finger does not silently become a
  // new drag, because a hand coming off a screen never lifts both at once.
  end(e) {
    const g = W.gesture;
    if (!g || g.state !== "scroll") return false;
    if (e.pointerId === g.id || e.pointerId === g.second) {
      // End the pinch before the axis. A toolkit that never sees the end keeps the gesture
      // open and ignores whatever comes next as part of it.
      if (g.pinchOn && g.pinchN) {
        W.sendInput({
          t: "pinch", phase: "up", x: g.pinchN.x, y: g.pinchN.y,
          scale: g.pinchScale, rotation: 0,
        });
        g.pinchOn = false;
      }
      // Terminate the axis. Without it a toolkit keeps waiting for more deltas and never
      // starts the kinetic phase, so a flick just stops dead where the finger left off.
      if (g.lastN) {
        W.sendInput({
          t: "scroll",
          x: g.lastN.x,
          y: g.lastN.y,
          dx: 0,
          dy: 0,
          source: "finger",
          stop: true,
        });
      }
      W.gesture = null;
      return true;
    }
    return false;
  },
};
