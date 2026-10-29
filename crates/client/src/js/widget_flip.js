// wado bridge — home's widget list never jumps. Whatever changes it — edit mode folding every
// body away, a widget stepped up or down, one added, one removed, a drag dropped — each widget
// glides from where it was to where it is now (FLIP: translate only, on the spring), a new one
// pops in, and a removed one shrinks and fades before the gap closes.
//
// "Where it was" is taken the moment a finger or a key goes down in the widgets' part of the
// page — every change here starts with one — and "where it is" when Rust has changed the DOM,
// which a MutationObserver sees in the same frame, before paint.
//
//   W.widgetFlip()     install, once the list is mounted (idempotent)
//   W.widgetGone(id)   shrink and fade one, then {type:"widgetRemove", id}
//   W.widgetPop(id)    a widget that just changed computer pops back from .96

(() => {
  const SPRING = "cubic-bezier(.32,.72,0,1)", POP = "cubic-bezier(.34,1.56,.64,1)", ACCEL = "cubic-bezier(.3,0,1,1)";
  const still = () => document.documentElement.dataset.motion === "off" || document.documentElement.dataset.motion === "reduced" ||
    matchMedia("(prefers-reduced-motion: reduce)").matches;
  let list = null;
  let tops = new Map();
  const snap = () => {
    tops = new Map();
    if (list) for (const el of list.children) if (el.dataset.id) tops.set(el.dataset.id, el.offsetTop);
  };
  const flip = () => {
    // A drop: the dragged widget and its neighbours already stand where the new order puts them.
    // Their inline offsets go in the same frame as the reorder, so nothing moves twice.
    if (W._wdrop) {
      W._wdrop = false;
      for (const el of list.children) { el.style.transition = "none"; el.style.translate = ""; el.classList.remove("shift", "settle", "lifted"); }
      requestAnimationFrame(() => { for (const el of list.children) el.style.transition = ""; });
      snap();
      return;
    }
    if (still()) return snap();
    for (const el of list.children) {
      const id = el.dataset.id;
      if (!id) continue;
      const was = tops.get(id);
      if (was === undefined) {
        if (tops.size) el.animate([{ scale: 0.85, opacity: 0 }, { scale: 1, opacity: 1 }], { duration: 420, easing: POP });
        continue;
      }
      const dy = was - el.offsetTop;
      if (Math.abs(dy) > 0.5) el.animate([{ translate: `0 ${dy}px` }, { translate: "0 0" }], { duration: 380, easing: SPRING });
    }
    snap();
  };

  W.widgetFlip = () => {
    const l = document.querySelector("#landing .wlist");
    if (!l || l._flip) return;
    list = l;
    snap();
    l._flip = new MutationObserver(flip);
    l._flip.observe(l, { childList: true, attributes: true, attributeFilter: ["class"] });
    // The list is re-made when home is; a fresh one installs itself again.
  };
  const before = (e) => {
    if (!list || !list.isConnected) { list = null; W.widgetFlip(); }
    if (e.target && e.target.closest && e.target.closest("#wsect, .wlist, .wtray, .wsheet, .wempty")) snap();
  };
  document.addEventListener("pointerdown", before, true);
  document.addEventListener("keydown", before, true);

  W.widgetGone = (id) => {
    const el = list && list.querySelector(`.widget[data-id="${CSS.escape(id)}"]`);
    const done = () => emit({ type: "widgetRemove", id });
    if (!el || still()) return done();
    el.style.pointerEvents = "none";
    el.animate([{ scale: 1, opacity: 1 }, { scale: 0.9, opacity: 0 }], { duration: 180, easing: ACCEL, fill: "forwards" })
      .finished.then(done, done);
  };

  W.widgetPop = (id) => {
    const el = list && list.querySelector(`.widget[data-id="${CSS.escape(id)}"]`);
    if (el && !still()) el.animate([{ scale: 0.96 }, { scale: 1 }], { duration: 360, easing: POP });
  };
})();
