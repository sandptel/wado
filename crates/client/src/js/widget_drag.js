// wado bridge — dragging a widget to a new place, in home's edit mode, by its ⋮⋮ grip.
//
// The lifted widget follows the finger 1:1 — no easing on direct manipulation — scaled to 1.03
// with the floating shadow (it now floats). The others slide out of its way on the spring, a
// light tick each time it crosses a slot. On release it settles into its slot with a little
// overshoot; then Rust reorders the list, and widget_flip.js drops the offsets in the same frame,
// so the DOM changes under it with no visible jump.
//
// To Rust: {type:"widgetPlace", from, to}.

(() => {
  let d = null;
  const still = () => document.documentElement.dataset.motion === "off" || matchMedia("(prefers-reduced-motion: reduce)").matches;
  const buzz = (ms) => { try { if (navigator.vibrate) navigator.vibrate(ms); } catch (_) {} };

  document.addEventListener("pointerdown", (e) => {
    const grip = e.target.closest && e.target.closest(".wlist.editing .grip");
    if (!grip || d) return;
    const el = grip.closest(".widget"), list = el.parentElement;
    const items = [...list.children].filter((x) => x.dataset.id);
    const rects = items.map((x) => x.getBoundingClientRect());
    const from = items.indexOf(el);
    if (from < 0) return;
    e.preventDefault();
    const gap = parseFloat(getComputedStyle(list).rowGap) || 8;
    // Rects and the finger are on the glass; a translate is in the page's own px, which the
    // interface size zooms (size.css).
    const z = el.currentCSSZoom || 1;
    d = { el, items, rects, from, to: from, h: rects[from].height / z + gap, y0: e.clientY, pid: e.pointerId, z };
    try { grip.setPointerCapture(e.pointerId); } catch (_) {}
    el.classList.add("lifted");
    for (const x of items) if (x !== el) x.classList.add("shift");
    buzz(8);
    grip.addEventListener("pointermove", move);
    grip.addEventListener("pointerup", end, { once: true });
    grip.addEventListener("pointercancel", end, { once: true });
  }, true);

  function move(e) {
    if (!d || e.pointerId !== d.pid) return;
    const { el, items, rects, from, h } = d;
    const dy = e.clientY - d.y0;
    el.style.translate = `0 ${dy / d.z}px`;
    const mid = rects[from].top + rects[from].height / 2 + dy;
    let to = from;
    rects.forEach((r, i) => {
      if (i < from && mid < r.top + r.height / 2) to = Math.min(to, i);
      if (i > from && mid > r.top + r.height / 2) to = i;
    });
    if (to !== d.to) { d.to = to; buzz(4); }
    items.forEach((x, i) => {
      if (x === el) return;
      let s = 0;
      if (from < to && i > from && i <= to) s = -h;
      if (from > to && i >= to && i < from) s = h;
      x.style.translate = s ? `0 ${s}px` : "";
    });
  }

  function end(e) {
    if (!d) return;
    const { el, items, rects, from, to, z } = d;
    e.target.removeEventListener("pointermove", move);
    d = null;
    // The slot it lands in, measured from where the widget now at `to` started.
    const target = to === from ? 0 : to > from ? rects[to].bottom - rects[from].bottom : rects[to].top - rects[from].top;
    el.classList.remove("lifted");
    el.classList.add("settle");
    el.style.translate = target ? `0 ${target / z}px` : "";
    const done = () => {
      if (to === from) {
        for (const x of items) { x.classList.remove("shift", "settle"); x.style.translate = ""; }
        return;
      }
      W._wdrop = true;
      emit({ type: "widgetPlace", from, to });
    };
    setTimeout(done, still() ? 0 : 340);
  }
})();
