// wado bridge — touchscreen gestures (pointerType "touch"/"pen"). A per-primary-contact FSM:
//   • plain press/drag        → wl_touch contact
//   • press-hold ~500ms still → retract (CancelTouch), then drag → window move, release →
//     right-click
//   • Move-mode on            → any drag is a window move
// Secondary simultaneous contacts pass straight through as wl_touch (multi-touch).

const touchAt = (id, phase, clientX, clientY, video) => {
  const n = W.normPoint(clientX, clientY, video);
  if (n) W.sendInput({ t: "touch", id: id >>> 0, phase, x: n.x, y: n.y });
};
const dragAt = (phase, clientX, clientY, video) => {
  const n = W.normPoint(clientX, clientY, video);
  if (n) W.sendInput({ t: "window_drag", phase, x: n.x, y: n.y });
};
// Primary-contact hold fired: retract the tap and arm hold (→ move or right-click).
const onHoldFired = () => {
  const g = W.gesture;
  if (!g || g.state !== "tap") return;
  g.state = "held";
  g.holdTimer = null;
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
    } else {
      touchAt(e.pointerId, "down", e.clientX, e.clientY, video); // secondary passthrough
    }
  },

  move(e, video) {
    const g = W.gesture;
    if (g && e.pointerId === g.id) {
      const dist = Math.hypot(e.clientX - g.startClientX, e.clientY - g.startClientY);
      if (g.state === "tap") {
        if (dist > MOVE_THRESHOLD) {
          if (g.holdTimer) { clearTimeout(g.holdTimer); g.holdTimer = null; }
          g.state = "touch";
          touchAt(g.id, "motion", e.clientX, e.clientY, video);
        }
      } else if (g.state === "touch") {
        touchAt(g.id, "motion", e.clientX, e.clientY, video);
      } else if (g.state === "held") {
        if (dist > MOVE_THRESHOLD) {
          g.state = "move";
          dragAt("down", g.startClientX, g.startClientY, video); // grab the original window
          dragAt("motion", e.clientX, e.clientY, video);
        }
      } else if (g.state === "move") {
        dragAt("motion", e.clientX, e.clientY, video);
      }
    } else {
      touchAt(e.pointerId, "motion", e.clientX, e.clientY, video);
    }
  },

  up(e, video) {
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
      W.gesture = null;
    } else {
      touchAt(e.pointerId, "up", e.clientX, e.clientY, video);
    }
  },
};
