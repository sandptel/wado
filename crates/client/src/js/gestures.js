// wado bridge — three-finger swipes: up, down, left, right, each mapped to an action by the
// daemon's config.kdl (`gestures { }`, sent in config_state) or the defaults below.
//
// Gesture mode only: in raw-touch mode three fingers are genuine multi-touch for the app.
// Recognised beside the one- and two-finger gestures, not inside them. The third finger only
// starts a swipe while the gesture already under way is still undecided ("pending" or "two"):
// once a drag or a pinch has committed, a third finger is ignored rather than tearing down
// something the app is in the middle of.

const SWIPE_MIN = 60; // CSS px the fingers' centre must travel to count
const DEFAULT_GESTURES = {
  "swipe-3-up": "app-drawer", "swipe-3-down": "control-centre",
  "swipe-3-left": "back", "swipe-3-right": "focus-next",
};

W.swipe3 = {
  active: false,
  pts: new Map(), // pointerId → { x0, y0, x, y }

  // Called on every touch pointerdown, before the gesture handlers. True: this event is now the
  // swipe's, and the gesture handlers must not see it.
  down(e) {
    if (this.active) { this.pts.set(e.pointerId, { x0: e.clientX, y0: e.clientY, x: e.clientX, y: e.clientY }); return true; }
    if (W.activePointers.size !== 3 || W.touchMode === "touch") return false;
    const g = W.gesture;
    if (g && g.state !== "pending" && g.state !== "two") return false;
    // Drop the undecided gesture: nothing was sent for it yet but hover motion.
    if (g && g.holdTimer) clearTimeout(g.holdTimer);
    W.gesture = null;
    this.active = true;
    this.pts.clear();
    this.pts.set(e.pointerId, { x0: e.clientX, y0: e.clientY, x: e.clientX, y: e.clientY });
    return true;
  },
  move(e) {
    if (!this.active) return false;
    const p = this.pts.get(e.pointerId);
    if (p) { p.x = e.clientX; p.y = e.clientY; }
    return true;
  },
  up(e) {
    if (!this.active) return false;
    if (W.activePointers.size > 0) return true; // wait for the last finger
    this.active = false;
    let dx = 0, dy = 0;
    for (const p of this.pts.values()) { dx += p.x - p.x0; dy += p.y - p.y0; }
    const n = Math.max(1, this.pts.size);
    dx /= n; dy /= n;
    if (Math.max(Math.abs(dx), Math.abs(dy)) < SWIPE_MIN) return true;
    const dir = Math.abs(dx) > Math.abs(dy) ? (dx > 0 ? "right" : "left") : (dy > 0 ? "down" : "up");
    const map = (W.hostConfig && W.hostConfig.gestures && Object.keys(W.hostConfig.gestures).length)
      ? W.hostConfig.gestures : DEFAULT_GESTURES;
    const action = map["swipe-3-" + dir];
    if (action && action !== "none") emit({ type: "gesture", action });
    return true;
  },
};
