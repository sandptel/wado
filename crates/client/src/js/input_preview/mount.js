// wado bridge — the Virtual input previews' one door (spec D6, §5.1, G15). Rust renders an empty box
// with `data-vi="<kind>"`; whichever preview registered that kind fills it when it appears, and its
// loop stops when the box goes (a page closed, a re-render). Loops are ambient: they run only
// while the box is on screen and motion is full, else they show one still frame.
//
//   W.inputPreview.register(kind, mount(el))   mount returns {demo?(name), stop?()}
//   W.inputPreview.demo(kind, name)            re-run one demo (a setting just changed)
//   W.inputPreview.running(el)                 the box is still on the page and visible
//   W.inputPreview.text(kind, words)           the preview's caption (`data-vi-demo`)

(() => {
  const P = (W.inputPreview = {});
  const kinds = {};
  const live = new Map(); // el → instance

  P.register = (kind, mount) => { kinds[kind] = mount; scan(); };
  P.level = () => (W.gpadMotion ? W.gpadMotion.level() : "full");
  P.running = (el) => el.isConnected && !el.closest("[hidden], [inert]") && el.getClientRects().length > 0;
  P.text = (kind, words) => {
    for (const c of document.querySelectorAll(`[data-vi-demo="${kind}"]`)) c.textContent = words;
  };
  P.demo = (kind, name) => {
    for (const [el, inst] of live) if (el.dataset.vi === kind && inst && inst.demo) inst.demo(name);
  };

  function scan() {
    for (const [el, inst] of live) {
      if (el.isConnected) continue;
      live.delete(el);
      if (inst && inst.stop) { try { inst.stop(); } catch (_) {} }
    }
    for (const el of document.querySelectorAll("[data-vi]")) {
      if (live.has(el) || !kinds[el.dataset.vi]) continue;
      let inst = null;
      try { inst = kinds[el.dataset.vi](el) || {}; } catch (e) { if (W.rlog) W.rlog("preview: " + e); }
      live.set(el, inst);
    }
  }
  let queued = false;
  const later = () => { if (queued) return; queued = true; requestAnimationFrame(() => { queued = false; scan(); }); };
  if (typeof MutationObserver === "function" && document.body) {
    new MutationObserver(later).observe(document.body, { childList: true, subtree: true });
  }
})();
