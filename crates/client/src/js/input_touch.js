// wado bridge — touchscreen gestures (pointerType "touch"/"pen"). A per-primary-contact FSM:
//   • plain press/drag        → wl_touch contact
//   • press-hold ~500ms still → retract (CancelTouch), then drag → window move, release →
//     right-click
//   • Move-mode on            → any drag is a window move
// Secondary simultaneous contacts pass straight through as wl_touch (multi-touch).

const touchAt = (id, phase, clientX, clientY, video) => {
  const n = W.normPoint(clientX, clientY, video);
  if (!n) return;
  const ev = { t: "touch", id: id >>> 0, phase, x: n.x, y: n.y };
  // Touch motion coalesces per contact; down/up are terminal and must keep their order.
  // Touch stays on the RELIABLE channel even for motion: a finger generates events at a
  // fraction of a mouse's rate, so it was never the saturation source, and a dropped
  // wl_touch motion for a live slot is messier than a dropped pointer motion.
  if (phase === "motion") W.coalesce.queue("touch:" + id, ev);
  else W.coalesce.now(ev);
};
const dragAt = (phase, clientX, clientY, video) => {
  const n = W.normPoint(clientX, clientY, video);
  if (!n) return;
  const ev = { t: "window_drag", phase, x: n.x, y: n.y };
  // Motion coalesces (newest position wins); down/up must arrive, and in order.
  if (phase === "motion") W.coalesce.queue("window_drag", ev);
  else W.coalesce.now(ev);
};
// Primary-contact hold fired: retract the tap and arm hold (→ move or right-click). The one
// gesture transition with no motion and nothing sent that a viewer could otherwise see, so it
// gets its own overlay mark — see `overlay.holdRing`.
const onHoldFired = () => {
  const g = W.gesture;
  if (!g || g.state !== "tap") return;
  g.state = "held";
  g.holdTimer = null;
  if (W.showTouches) W.overlay.holdRing(g.startClientX, g.startClientY);
  W.sendInput({ t: "cancel_touch", id: g.id >>> 0 });
};

W.touchg = {
  down(e, video) {
    if (W.gesture === null) {
      const g = { id: e.pointerId, startClientX: e.clientX, startClientY: e.clientY, holdTimer: null };
      if (W.moveMode) {
        g.state = "move";
        dragAt("down", e.clientX, e.clientY, video);
      } else {
        g.state = "tap";
        touchAt(e.pointerId, "down", e.clientX, e.clientY, video);
        g.holdTimer = setTimeout(onHoldFired, HOLD_MS);
      }
      W.gesture = g;
    } else if (W.scrollg.begin(e, video)) {
      // A second contact means scrolling, not multi-touch. Deliberately ahead of the
      // passthrough: two fingers on a desktop app are far more often a scroll than a
      // genuine multi-touch gesture, and the toolkits that want raw multi-touch are the
      // rarer case. See input_scroll.js.
    } else {
      touchAt(e.pointerId, "down", e.clientX, e.clientY, video); // secondary passthrough
    }
  },

  move(e, video) {
    if (W.scrollg.move(e, video)) return;
    const g = W.gesture;
    if (g && e.pointerId === g.id) {
      // The scroll midpoint is computed from both contacts, so the primary's latest
      // position has to be known at the moment the second one lands.
      g.lastClientX = e.clientX;
      g.lastClientY = e.clientY;
      const dist = Math.hypot(e.clientX - g.startClientX, e.clientY - g.startClientY);
      if (g.state === "tap") {
        if (dist > MOVE_THRESHOLD) {
          if (g.holdTimer) { clearTimeout(g.holdTimer); g.holdTimer = null; }
          g.state = "touch";
          touchAt(g.id, "motion", e.clientX, e.clientY, video);
          if (W.showTouches) W.overlay.trail(g.id, e.clientX, e.clientY);
        }
      } else if (g.state === "touch") {
        touchAt(g.id, "motion", e.clientX, e.clientY, video);
        if (W.showTouches) W.overlay.trail(g.id, e.clientX, e.clientY);
      } else if (g.state === "held") {
        if (dist > MOVE_THRESHOLD) {
          g.state = "move";
          dragAt("down", g.startClientX, g.startClientY, video); // grab the original window
          dragAt("motion", e.clientX, e.clientY, video);
          if (W.showTouches) W.overlay.trail(g.id, g.startClientX, g.startClientY);
        }
      } else if (g.state === "move") {
        dragAt("motion", e.clientX, e.clientY, video);
        if (W.showTouches) W.overlay.trail(g.id, e.clientX, e.clientY);
      }
    } else {
      // Secondary contact — usually a scroll gesture (see W.scrollg above), which returns
      // early; reaching here means genuine multi-touch passthrough, and it swipes too.
      touchAt(e.pointerId, "motion", e.clientX, e.clientY, video);
      if (W.showTouches) W.overlay.trail(e.pointerId, e.clientX, e.clientY);
    }
  },

  up(e, video) {
    if (W.scrollg.end(e)) return;
    const g = W.gesture;
    if (g && e.pointerId === g.id) {
      if (g.holdTimer) { clearTimeout(g.holdTimer); g.holdTimer = null; }
      if (g.state === "tap" || g.state === "touch") {
        touchAt(g.id, "up", e.clientX, e.clientY, video);
      } else if (g.state === "move") {
        dragAt("up", e.clientX, e.clientY, video);
      } else if (g.state === "held") {
        const n = W.normPoint(e.clientX, e.clientY, video); // press-hold-release → right-click
        if (n) {
          W.sendInput({ t: "button", x: n.x, y: n.y, button: "right", pressed: true });
          W.sendInput({ t: "button", x: n.x, y: n.y, button: "right", pressed: false });
        }
      }
      if (W.showTouches) W.overlay.trailEnd(g.id);
      W.gesture = null;
    } else {
      touchAt(e.pointerId, "up", e.clientX, e.clientY, video);
      if (W.showTouches) W.overlay.trailEnd(e.pointerId);
    }
  },
};
