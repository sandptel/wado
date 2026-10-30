// wado bridge — the Pocket Console's slots: tells Rust what the layout is computed from, and
// writes back what it computed (`ui/surround/layout.rs`) as CSS variables on the root.
//
// Scale, never resize (spec §2.1): the console moves and scales `#wado-video` with one transform
// (`surround.css`, from `--c-tx/--c-ty/--c-k`); the session's output never changes size, so no
// fresh Output, no IDR, no app reflow. Input reads the video's *transformed* rect at event time
// (`W.normPoint`, `W.cssToLogical`: `getBoundingClientRect` includes transforms), so a tap lands
// where the finger is in either arrangement and mid-morph.
//
//   W.stageFit.report()        say the viewport, its safe insets, the form and the video's size
//   W.stageFit.apply(L)        Rust's layout: {w, h, form, face, ws, nav, faces, deck, vars}
//   W.stageFit.layout          the last one applied
//   W.stageFit.next(ms)        a promise for the next layout applied (or the timeout)
//   W.stageFit.slot(name)      one slot as {x, y, w, h, shown} in CSS px, or null — the seam an
//                              input face lays itself out from (the gamepad's box is `pad`)

(() => {
  const F = (W.stageFit = { layout: null });
  const root = document.documentElement;
  const waiting = [];

  // env(safe-area-inset-*) is only readable through a property that takes it.
  let probe = null;
  function insets() {
    if (!probe || !probe.isConnected) {
      probe = document.createElement("div");
      probe.style.cssText = "position:fixed;visibility:hidden;pointer-events:none;inset:0 auto auto 0;" +
        "padding:env(safe-area-inset-top) env(safe-area-inset-right) env(safe-area-inset-bottom) env(safe-area-inset-left)";
      document.body.appendChild(probe);
    }
    const cs = getComputedStyle(probe);
    return { t: parseFloat(cs.paddingTop) || 0, r: parseFloat(cs.paddingRight) || 0,
      b: parseFloat(cs.paddingBottom) || 0, l: parseFloat(cs.paddingLeft) || 0 };
  }

  // Spec §4: the short side under 600 is a phone; above it a coarse pointer is a tablet.
  F.form = (w, h) => {
    if (Math.min(w, h) < 600) return w > h ? "land" : "port";
    let coarse = false;
    try { coarse = matchMedia("(pointer: coarse)").matches; } catch (_) {}
    if (coarse) return w > h ? "tland" : "tport";
    return "desk";
  };

  let last = "";
  F.report = () => {
    const w = innerWidth, h = innerHeight;
    const v = document.getElementById("wado-video");
    const vw = (v && v.videoWidth) || 0, vh = (v && v.videoHeight) || 0;
    const sa = insets();
    const msg = { type: "stageView", w, h, ...sa, form: F.form(w, h), vw, vh };
    const key = JSON.stringify(msg);
    if (key === last) return;
    last = key;
    emit(msg);
  };

  // Slots a layout can leave empty (o = 0): no Tab stop on a control that is not there.
  const HIDE = { rotate: ".rail [data-rotate]", kbd: ".rail [data-osk]", strip: ".cstrip", deck: ".cdeck" };

  F.apply = (L) => {
    if (!L || !L.vars) return;
    F.layout = L;
    for (const k in L.vars) root.style.setProperty(k, L.vars[k]);
    root.dataset.form = L.form;
    root.dataset.cws = L.ws;
    root.dataset.cnav = L.nav;
    root.dataset.cfaces = L.faces;
    root.dataset.cdeck = L.deck;
    root.dataset.face = L.face;
    F.inert();
    // The touchpad's pointer sits over the picture from the start, not only once touched.
    if (W.deckPad) requestAnimationFrame(W.deckPad.refit);
    // A relayout someone asked to animate (a face change): fly from where things were.
    if (W.stageMorph && W.stageMorph.pending) W.stageMorph.settle();
    while (waiting.length) waiting.shift()(L);
  };

  // Controls the console leaves empty, and everything that belongs to the other arrangement.
  F.inert = () => {
    const arr = root.dataset.arr;
    const vars = (F.layout && F.layout.vars) || {};
    for (const k in HIDE) {
      const off = arr === "console" && vars[`--c-${k}-o`] === "0";
      document.querySelectorAll(HIDE[k]).forEach((el) => el.toggleAttribute("inert", off));
    }
    document.querySelectorAll("[data-only]").forEach((el) => {
      el.toggleAttribute("inert", !!arr && el.dataset.only !== arr);
    });
  };

  F.slot = (k) => {
    const v = (F.layout && F.layout.vars) || {};
    const n = (s) => parseFloat(v[`--c-${k}-${s}`]);
    if (!(`--c-${k}-y` in v)) return null;
    return { x: n("x"), y: n("y"), w: n("w"), h: n("h"), shown: v[`--c-${k}-o`] !== "0" };
  };

  F.next = (ms) => new Promise((resolve) => {
    waiting.push(resolve);
    setTimeout(() => resolve(null), ms || 400);
  });

  addEventListener("resize", F.report);
  addEventListener("orientationchange", () => setTimeout(F.report, 120));
  if (typeof visualViewport !== "undefined" && visualViewport) visualViewport.addEventListener("resize", F.report);
  // The session's aspect arrives with its first frame, and changes when a joiner's lead turns.
  document.addEventListener("resize", (e) => { if (e.target && e.target.id === "wado-video") F.report(); }, true);
  document.addEventListener("loadedmetadata", (e) => { if (e.target && e.target.id === "wado-video") F.report(); }, true);
  // The link pill's width is its words'; the name sits after it.
  if (typeof ResizeObserver !== "undefined") {
    const ro = new ResizeObserver((es) => {
      for (const e of es) root.style.setProperty("--c-linkw", Math.ceil(e.target.getBoundingClientRect().width) + "px");
    });
    let watched = null;
    setInterval(() => {
      const p = document.querySelector(".linkbadge .linkpill");
      if (p === watched) return;
      if (watched) ro.unobserve(watched);
      watched = p;
      if (p) ro.observe(p);
      else root.style.setProperty("--c-linkw", "0px");
    }, 500);
  }
  F.report();
})();
