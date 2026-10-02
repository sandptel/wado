// wado bridge — touch as a TRANSLATED POINTER (touch mode "pointer", the default). The raw
// wl_touch mode is input_touch.js.
//
// Why translate at all. Most desktop applications do not speak wl_touch well — anything under
// Xwayland, Electron, older Qt — so a finger never produced a double-click, a right-click or a
// scroll there. Interpreting the gesture here and sending pointer events makes every
// application behave the same, which is the whole point on a phone. The cost, accepted in the
// Decision Log (2026-09-29): an application's own touch features are replaced by these.
//
//   tap                 → left click, at the finger's down point
//   second tap ≤300 ms  → left click snapped onto the first tap's point, so the toolkit sees a
//                         double-click (a finger jitters further than its few-pixel tolerance)
//   hold still ~500 ms  → release: right click. Drag instead: pick the item up — left pressed
//                         where the hold began, pointer following the finger (select, drag
//                         files), the way a phone's long-press-and-drag works
//   one-finger drag     → finger-source scroll; the axis-stop on lift starts toolkit kinetics
//   two fingers         → whichever commits first: the gap changing is a pinch (handed to
//                         W.scrollg, as in raw mode), the primary moving is a press-and-drag —
//                         left held, pointer following the primary finger (select text, DnD)
//   Move-mode on        → any drag is a window move
//
// Nothing is sent on touch-down except a hover motion: until the finger lifts or moves, it is
// not yet known whether this is a tap, a scroll or a hold, and a press sent early would have
// to be retracted.

const DOUBLE_TAP_MS = 300;
// CSS px between two taps that still counts as the same spot. Generous on purpose: this is
// fingertip-sized, and the snap is what makes it land as a double-click.
const DOUBLE_TAP_RADIUS = 24;
// Fraction the gap between two fingers must change by before it reads as a pinch rather
// than two fingers resting while the primary drags.
const PINCH_COMMIT = 0.12;

const clickAt = (n, button) => {
  W.coalesce.now({ t: "button", x: n.x, y: n.y, button, pressed: true });
  W.sendInput({ t: "button", x: n.x, y: n.y, button, pressed: false });
};

const onTapHold = () => {
  const g = W.gesture;
  if (!g || g.state !== "pending") return;
  g.state = "held";
  g.holdTimer = null;
  if (W.showTouches) W.overlay.holdRing(g.startClientX, g.startClientY);
};

W.touchp = {
  // Last completed tap, for the double-tap snap: { t, clientX, clientY, n }.
  lastTap: null,

  down(e, video) {
    const g = W.gesture;
    if (g === null) {
      const ng = {
        id: e.pointerId, startClientX: e.clientX, startClientY: e.clientY,
        lastClientX: e.clientX, lastClientY: e.clientY, holdTimer: null,
      };
      if (W.moveMode) {
        ng.state = "move";
        W.windowDragAt("down", e.clientX, e.clientY, video);
      } else {
        ng.state = "pending";
        ng.holdTimer = setTimeout(onTapHold, HOLD_MS);
        // Hover only: lets the application show what is under the finger before anything
        // is committed. A pointer motion can never need retracting.
        const n = W.normPoint(e.clientX, e.clientY, video);
        if (n) W.coalesce.queue("pointer_motion", { t: "pointer_motion", x: n.x, y: n.y });
        // Ask now what is under the finger, so the answer is back by the time it lifts.
        if (W.targets) ng.ticket = W.targets.ask(e.clientX, e.clientY, video);
      }
      W.gesture = ng;
      return;
    }
    // Pinch, owned by the two-finger scroll module. A further contact changes nothing.
    if (g.state === "scroll" || g.second != null || g.state === "hold-drag") return;
    if (g.holdTimer) { clearTimeout(g.holdTimer); g.holdTimer = null; }
    if (g.state === "pan") W.fingerScroll.stop(g.lastN);
    else if (g.state === "move") W.windowDragAt("up", g.lastClientX, g.lastClientY, video);
    g.state = "two";
    g.second = e.pointerId;
    g.p1 = { x: g.lastClientX, y: g.lastClientY };
    g.p1Start = { ...g.p1 };
    g.p2 = { x: e.clientX, y: e.clientY };
    g.gap0 = Math.max(1, Math.hypot(g.p2.x - g.p1.x, g.p2.y - g.p1.y));
  },

  move(e, video) {
    if (W.scrollg.move(e, video)) return; // a committed pinch
    const g = W.gesture;
    if (!g) return;

    if (g.state === "two" || g.state === "drag2") {
      if (e.pointerId === g.id) g.p1 = { x: e.clientX, y: e.clientY };
      else if (e.pointerId === g.second) g.p2 = { x: e.clientX, y: e.clientY };
      else return;
      if (g.state === "drag2") {
        if (e.pointerId !== g.id) return; // the pointer follows the primary finger only
        const n = W.normPoint(e.clientX, e.clientY, video);
        if (n) W.coalesce.queue("pointer_motion", { t: "pointer_motion", x: n.x, y: n.y });
        if (W.showTouches) W.overlay.trail(g.id, e.clientX, e.clientY);
        return;
      }
      const gap = Math.hypot(g.p2.x - g.p1.x, g.p2.y - g.p1.y);
      if (Math.abs(gap / g.gap0 - 1) > PINCH_COMMIT) {
        // Hand over to the two-finger module, which pinches and scrolls from here.
        g.state = "pinch-handover";
        g.lastClientX = g.p1.x;
        g.lastClientY = g.p1.y;
        W.scrollg.begin({ pointerId: g.second, clientX: g.p2.x, clientY: g.p2.y }, video);
      } else if (Math.hypot(g.p1.x - g.p1Start.x, g.p1.y - g.p1Start.y) > MOVE_THRESHOLD) {
        const n0 = W.normPoint(g.p1Start.x, g.p1Start.y, video);
        if (!n0) return;
        g.state = "drag2";
        W.coalesce.now({ t: "button", x: n0.x, y: n0.y, button: "left", pressed: true });
        const n = W.normPoint(g.p1.x, g.p1.y, video);
        if (n) W.coalesce.queue("pointer_motion", { t: "pointer_motion", x: n.x, y: n.y });
      }
      return;
    }

    if (e.pointerId !== g.id) return;
    const dx = e.clientX - g.lastClientX;
    const dy = e.clientY - g.lastClientY;
    const dist = Math.hypot(e.clientX - g.startClientX, e.clientY - g.startClientY);
    if (g.state === "pending") {
      if (dist <= MOVE_THRESHOLD) return; // not yet committed; keep the last point where it is
      if (g.holdTimer) { clearTimeout(g.holdTimer); g.holdTimer = null; }
      g.state = "pan";
      // From the down point, so the threshold's worth of travel is scrolled rather than lost.
      const n = W.fingerScroll.delta(
        e.clientX - g.startClientX, e.clientY - g.startClientY, e.clientX, e.clientY, video);
      if (n) g.lastN = n;
    } else if (g.state === "pan") {
      const n = W.fingerScroll.delta(dx, dy, e.clientX, e.clientY, video);
      if (n) g.lastN = n;
    } else if (g.state === "held") {
      if (dist <= MOVE_THRESHOLD) return;
      const n0 = W.normPoint(g.startClientX, g.startClientY, video);
      if (!n0) return;
      g.state = "hold-drag";
      W.coalesce.now({ t: "button", x: n0.x, y: n0.y, button: "left", pressed: true });
      const n = W.normPoint(e.clientX, e.clientY, video);
      if (n) W.coalesce.queue("pointer_motion", { t: "pointer_motion", x: n.x, y: n.y });
    } else if (g.state === "hold-drag") {
      const n = W.normPoint(e.clientX, e.clientY, video);
      if (n) W.coalesce.queue("pointer_motion", { t: "pointer_motion", x: n.x, y: n.y });
    } else if (g.state === "move") {
      W.windowDragAt("motion", e.clientX, e.clientY, video);
    }
    g.lastClientX = e.clientX;
    g.lastClientY = e.clientY;
    if (W.showTouches && g.state !== "held") W.overlay.trail(g.id, e.clientX, e.clientY);
  },

  up(e, video) {
    if (W.scrollg.end(e)) return; // a committed pinch
    const g = W.gesture;
    if (!g) return;
    // Two fingers: either one lifting ends the gesture, as in raw mode — a hand never lifts
    // both at once, and a leftover finger must not silently become a new drag.
    if (g.state === "two" || g.state === "drag2") {
      if (e.pointerId !== g.id && e.pointerId !== g.second) return;
      if (g.state === "drag2") {
        const n = W.normPoint(g.p1.x, g.p1.y, video);
        if (n) W.coalesce.now({ t: "button", x: n.x, y: n.y, button: "left", pressed: false });
      } else if (W.lens) {
        // Two fingers down and up without moving: the lens, on demand, between them.
        W.lens.open((g.p1.x + g.p2.x) / 2, (g.p1.y + g.p2.y) / 2);
      }
      if (W.showTouches) W.overlay.trailEnd(g.id);
      W.gesture = null;
      return;
    }
    if (e.pointerId !== g.id) return;
    if (g.holdTimer) { clearTimeout(g.holdTimer); g.holdTimer = null; }
    if (g.state === "pending") {
      this.tap(g, video);
    } else if (g.state === "pan") {
      W.fingerScroll.stop(g.lastN);
    } else if (g.state === "held") {
      const n = W.normPoint(g.startClientX, g.startClientY, video);
      if (n) clickAt(n, "right");
    } else if (g.state === "hold-drag") {
      const n = W.normPoint(e.clientX, e.clientY, video);
      if (n) W.coalesce.now({ t: "button", x: n.x, y: n.y, button: "left", pressed: false });
    } else if (g.state === "move") {
      W.windowDragAt("up", e.clientX, e.clientY, video);
    }
    if (W.showTouches) W.overlay.trailEnd(g.id);
    W.gesture = null;
  },

  tap(g, video) {
    const now = performance.now();
    const prev = this.lastTap;
    let n;
    if (prev && now - prev.t <= DOUBLE_TAP_MS &&
        Math.hypot(g.startClientX - prev.clientX, g.startClientY - prev.clientY) <= DOUBLE_TAP_RADIUS) {
      n = prev.n; // the snap: same logical point, so the toolkit counts a double (or triple)
    } else {
      n = W.normPoint(g.startClientX, g.startClientY, video);
    }
    if (!n) return;
    const snapped = prev && n === prev.n;
    // Chained from the first tap's position, so a triple-tap stays on the same spot too.
    // Recorded before any waiting below, so a quick second tap already sees it.
    this.lastTap = {
      t: now,
      clientX: snapped ? prev.clientX : g.startClientX,
      clientY: snapped ? prev.clientY : g.startClientY,
      n,
    };
    // The second tap of a double is already aimed — at the first. Only a fresh tap consults
    // the targets (see targets.js), and only when that machinery is loaded.
    if (snapped || g.ticket == null || !W.targets) { clickAt(n, "left"); return; }
    const cx = g.startClientX, cy = g.startClientY;
    W.targets.wait(g.ticket).then((targets) => {
      const k = W.targets.content(video);
      if (targets === null) {
        // No tree for this app: judge the pixels instead.
        if (W.lensAuto && W.lens && W.lens.dense(cx, cy)) W.lens.open(cx, cy);
        else clickAt(n, "left");
        return;
      }
      const size = (t) => ({ w: t.w * (k ? k.w : 1), h: t.h * (k ? k.h : 1) });
      const d = W.decideTap(n, targets, size);
      if (d.kind === "lens" && W.lens && W.lensAuto) W.lens.open(cx, cy);
      else clickAt(d.at || n, "left");
    });
  },
};
