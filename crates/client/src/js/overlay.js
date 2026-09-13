// wado bridge — debug "show touches" overlay. A transparent, pointer-events:none canvas
// pinned over the viewport, drawing three distinct things so a recording or a screen-share of
// the phone reads back as what actually happened, not just where fingers landed:
//
//   mark()      a tap/click — a short dot at the down point.
//   trail()     a swipe/drag — a fading stroke connecting consecutive move points, so a swipe
//               reads as the swirl it actually was instead of a scatter of disconnected dots.
//               Keyed per contact id, because two fingers (or a finger and the mouse) mid-drag
//               must not have their positions connected to each other.
//   holdRing()  a long-press *firing* — the moment `input_touch.js`'s HOLD_MS timer promotes a
//               tap to a hold gesture, which is otherwise invisible: nothing moves, nothing is
//               sent, so a plain dot gives no sign that anything happened at all.
//
// Purely client-side, purely visual — never touches what gets sent to the compositor.

W.overlay = {
  el: null,
  // Last drawn point per contact id, so `trail()` knows what to connect to. Cleared per-id on
  // `trailEnd()` (pointer up) so a fresh drag never draws a stray line back to wherever the
  // previous one ended.
  trails: {},

  ensure() {
    const c0 = W.overlay.el;
    const fresh = !c0;
    const c = fresh ? document.createElement("canvas") : c0;
    if (fresh) {
      c.id = "wado-touch-overlay";
      c.style.cssText = "position:fixed;left:0;top:0;pointer-events:none;z-index:9999;";
      document.body.appendChild(c);
      W.overlay.el = c;
    }
    // Assigning width/height clears the canvas as a side effect — matched here by dropping
    // the trail state too, since old points would otherwise be in stale pixel coordinates
    // after a viewport resize (rotation, resize, address-bar show/hide).
    if (c.width !== window.innerWidth || c.height !== window.innerHeight) {
      c.width = window.innerWidth;
      c.height = window.innerHeight;
      W.overlay.trails = {};
    }
    return c;
  },

  mark(clientX, clientY) {
    if (!W.showTouches) return;
    const c = W.overlay.ensure();
    const ctx = c.getContext("2d");
    ctx.beginPath();
    ctx.arc(clientX, clientY, 18, 0, Math.PI * 2);
    ctx.fillStyle = "rgba(46,200,120,0.45)";
    ctx.fill();
    setTimeout(() => { try { ctx.clearRect(clientX - 20, clientY - 20, 40, 40); } catch (_) {} }, 220);
  },

  trail(id, clientX, clientY) {
    if (!W.showTouches) return;
    const c = W.overlay.ensure();
    const ctx = c.getContext("2d");
    const last = W.overlay.trails[id];
    if (last) {
      ctx.beginPath();
      ctx.moveTo(last.x, last.y);
      ctx.lineTo(clientX, clientY);
      ctx.strokeStyle = "rgba(64,170,255,0.55)";
      ctx.lineWidth = 6;
      ctx.lineCap = "round";
      ctx.stroke();
    }
    W.overlay.trails[id] = { x: clientX, y: clientY };
    // Fade just this segment's own footprint, the same pattern `mark()` uses — a stroke drawn
    // a beat later in the same spot can get its edge clipped by an older segment's clearRect,
    // which is a cosmetic imperfection this debug overlay accepts rather than maintaining a
    // full redraw buffer for.
    const x0 = Math.min(last ? last.x : clientX, clientX) - 12;
    const y0 = Math.min(last ? last.y : clientY, clientY) - 12;
    const x1 = Math.max(last ? last.x : clientX, clientX) + 12;
    const y1 = Math.max(last ? last.y : clientY, clientY) + 12;
    setTimeout(() => { try { ctx.clearRect(x0, y0, x1 - x0, y1 - y0); } catch (_) {} }, 280);
  },

  // Pointer up / contact lifted: forget its last point so the next drag from this id starts a
  // new stroke instead of connecting back to wherever this one ended.
  trailEnd(id) {
    delete W.overlay.trails[id];
  },

  // A long-press firing is a decision, not a movement — nothing else on screen marks the
  // moment `onHoldFired()` promotes a tap to a hold. A short rAF-driven ring (bounded to one
  // ~450ms burst, not an ongoing loop) is the cheapest way to say "this is what changed the
  // gesture" without adding a persistent per-frame cost the rest of the time.
  holdRing(clientX, clientY) {
    if (!W.showTouches) return;
    const c = W.overlay.ensure();
    const ctx = c.getContext("2d");
    const start = performance.now();
    const DURATION = 450;
    const pad = 8;
    const step = (now) => {
      const t = Math.min(1, (now - start) / DURATION);
      const r = 14 + 46 * t;
      // Clear the ring's own last footprint before drawing the next frame's — this canvas has
      // no other persistent content near a hold point, so an unconditional clearRect here is
      // safe (unlike trail(), which shares the region with other strokes).
      ctx.clearRect(clientX - r - pad, clientY - r - pad, (r + pad) * 2, (r + pad) * 2);
      ctx.beginPath();
      ctx.arc(clientX, clientY, r, 0, Math.PI * 2);
      ctx.strokeStyle = `rgba(255,150,40,${(0.75 * (1 - t)).toFixed(3)})`;
      ctx.lineWidth = 4;
      ctx.stroke();
      if (t < 1) requestAnimationFrame(step);
      else { try { ctx.clearRect(clientX - r - pad, clientY - r - pad, (r + pad) * 2, (r + pad) * 2); } catch (_) {} }
    };
    requestAnimationFrame(step);
  },

  clear() {
    const c = W.overlay.el;
    if (c) { const ctx = c.getContext("2d"); if (ctx) ctx.clearRect(0, 0, c.width, c.height); }
    W.overlay.trails = {};
  },
};

W.setShowTouches = (on) => {
  W.showTouches = !!on;
  if (!on) W.overlay.clear();
};
