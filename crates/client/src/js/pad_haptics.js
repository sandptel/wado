// wado bridge — what the hand feels on the touchpad (port of the app's pad_haptics.dart): the
// moment the computer will read something from the fingers — a click, a right click, a drag
// taking hold, a swipe taking over, the point a Space or Mission Control commits — each its own
// feel. Read here with the computer's thresholds, so it never waits on the network. In the app's
// WebView the phone's own effects; in a browser, `navigator.vibrate` patterns (length stands in
// for strength). After pad_touch.js (it wraps `W.padTouch.input`).
(() => {
  if (!W.padTouch || !W.padTouch.input) return;
  const VIBRATE = {
    click: 12, right: [8, 40, 10], grab: 28, drop: 6, scroll: 4, zoom: 4,
    swipe: [5, 20, 10], pinch: [5, 30, 14], detent: 18, undo: 5,
  };
  const KIND = { click: "tap", right: "reject", grab: "heavy", detent: "tap" };
  let sink = null; // a test's recorder, in place of the motor
  const feel = (f) => {
    if (sink) sink(f);
    else if (W.native && W.native.haptic) W.native.haptic(KIND[f] || "tick");
    else if (navigator.vibrate) try { navigator.vibrate(VIBRATE[f]); } catch (_) {}
  };
  const TAP_MS = 180, TAP_SLOP = 8, DRAG_WINDOW = 250, SLOP = 10, SHARE = 0.5, ZOOM_STEP = 1.12;

  const at = new Map();
  let g = "idle", began = 0, most = 0, moved = 0, lastTap = null, from = [0, 0], spread0 = 0,
    zoomRef = 0, axis = "", zone = 0, w = 400, h = 300;
  const centroid = () => {
    let x = 0, y = 0;
    for (const p of at.values()) { x += p[0]; y += p[1]; }
    return [x / at.size, y / at.size];
  };
  const spread = () => {
    const c = centroid();
    let s = 0;
    for (const p of at.values()) s += Math.hypot(p[0] - c[0], p[1] - c[1]);
    return s / at.size;
  };
  const dock = () => {
    const c = centroid(), dx = c[0] - from[0], dy = c[1] - from[1];
    const p = axis === "h" ? dx / (w * SHARE) : axis === "v" ? dy / (h * SHARE) : (spread() - spread0) / (w * SHARE * 0.5);
    const z = Math.floor(Math.abs(p) + 0.5);
    if (z > zone) feel("detent");
    if (z < zone) feel("undo");
    zone = z;
  };

  function down(id, x, y, now) {
    const first = at.size === 0;
    at.set(id, [x, y]);
    if (first) {
      began = now; moved = 0; most = 1;
      g = lastTap != null && now - lastTap < DRAG_WINDOW ? "armed" : "point";
      return;
    }
    most = Math.max(most, at.size);
    if (moved >= TAP_SLOP) return;
    if (at.size === 2 && g === "point") g = "two";
    else if (at.size >= 3 && (g === "point" || g === "two" || g === "many")) g = "many";
    else return;
    from = centroid();
    spread0 = spread();
  }

  function move(id, x, y) {
    const was = at.get(id);
    if (!was) return;
    at.set(id, [x, y]);
    moved += Math.hypot(x - was[0], y - was[1]) / at.size;
    if (g === "armed" && moved > 3) { feel("grab"); g = "drag"; }
    else if (g === "two" && at.size === 2) {
      const c = centroid(), travel = Math.hypot(c[0] - from[0], c[1] - from[1]), change = Math.abs(spread() - spread0);
      if (change >= TAP_SLOP && change > travel) { g = "zoom"; zoomRef = spread(); feel("zoom"); }
      else if (travel >= TAP_SLOP) { g = "scroll"; feel("scroll"); }
    } else if (g === "zoom") {
      const r = spread() / Math.max(zoomRef, 1);
      if (r >= ZOOM_STEP || r <= 1 / ZOOM_STEP) { zoomRef = spread(); feel("zoom"); }
    } else if (g === "many") {
      const c = centroid(), dx = c[0] - from[0], dy = c[1] - from[1], grew = spread() - spread0, travel = Math.hypot(dx, dy);
      if (Math.max(travel, Math.abs(grew)) < SLOP) return;
      axis = at.size >= 4 && Math.abs(grew) > travel ? "pinch" : Math.abs(dx) >= Math.abs(dy) ? "h" : "v";
      g = "dock"; zone = 0;
      feel(axis === "pinch" ? "pinch" : "swipe");
      dock();
    } else if (g === "dock") dock();
  }

  function up(id, now) {
    if (!at.delete(id)) return;
    if (at.size) {
      // A one-finger drag or point goes on, and a two-finger tap whose fingers lift one after
      // the other is still a tap; anything else is over.
      if (!["point", "armed", "drag", "two"].includes(g)) g = "done";
      return;
    }
    const quick = now - began < TAP_MS && moved < TAP_SLOP;
    if (g === "drag") { feel("drop"); lastTap = null; }
    else if ((g === "point" || g === "armed") && quick && most === 1) { feel("click"); lastTap = g === "armed" ? null : now; }
    else if (g === "two" && quick && most === 2) { feel("right"); lastTap = null; }
    else lastTap = null;
    g = "idle";
  }

  const input = W.padTouch.input;
  W.padTouch.input = (type, p) => {
    const now = performance.now();
    if (p.w) { w = p.w; h = p.h; }
    if (type === "down") down(p.id, p.x, p.y, now);
    else if (type === "move") move(p.id, p.x, p.y);
    else if (type === "up") up(p.id, now);
    return input(type, p);
  };
  // For a headless check: the model alone, fed by hand, its feels recorded.
  W.padHaptics = { down, move, up, record(fn) { sink = fn; } };
})();
