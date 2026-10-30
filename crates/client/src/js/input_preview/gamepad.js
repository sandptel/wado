// wado bridge — the Gamepad settings page's live parts (spec §5.4): the pad itself, pressable, with
// what the computer would see (presses here go nowhere — the caption says what they would send);
// the layout carousels (the real renderer at thumbnail size; tapping one morphs the preview, G6);
// and the sticks' type and dead zone, which belong to the layout and are saved with it.
//
//   W.inputPreview.gamepadSet("stick" | "dead", v)   on the layout used away from a session

(() => {
  const IP = W.inputPreview;
  let preview = null; // {el, ctl, L}

  function layout() { return W.gpadPrefs.layoutFor("away"); }

  function draw(el, L, morph) {
    const w = el.clientWidth, h = el.clientHeight;
    if (!w || !h) return;
    const first = morph && el._gp ? W.gpadMotion.rects(el._gp) : null;
    const ghosts = first ? W.gpadMotion.ghosts(el, el._gp, new Set(L.controls.filter((c) => !c.hidden).map((c) => c.id))) : [];
    const k = W.gpadPlace.thumbK("l", w, h);
    const place = W.gpadPlace.all(L, "l", k, w, W.gpadVariants.REF.l[1] * k);
    for (const r of place.values()) r.y += (h - W.gpadVariants.REF.l[1] * k) / 2;
    // A picture you can press: the presses go nowhere, so it is not a control to a screen reader
    // (the page's own settings are), and its thumbnail-size buttons are not targets.
    W.gpadRender(el, L, { o: "l", k, live: false, place });
    if (first) W.gpadMotion.morph(el, first, ghosts);
  }

  IP.register("gamepad", (el) => {
    const ctl = W.gpadController(null);
    preview = { el, ctl, L: layout() };
    ctl.onChange(() => IP.text("gamepad", ctl.sees().join(" + ") || "—"));
    W.gpadInput.attach(el, {
      get ctl() { return ctl; },
      layout: () => preview.L,
      haptics: () => W.gpadPrefs.get().haptics,
      editing: () => false,
    });
    requestAnimationFrame(() => { draw(el, preview.L); if (el._gp) W.gpadMotion.assemble(el._gp, "l"); });
    const ro = typeof ResizeObserver === "function" ? new ResizeObserver(() => el.isConnected && draw(el, preview.L)) : null;
    if (ro) ro.observe(el);
    return { stop() { if (ro) ro.disconnect(); ctl.releaseAll(); if (preview && preview.el === el) preview = null; } };
  });

  // A change of layout anywhere (a carousel, the editor, another browser) morphs the preview.
  W.gpadPrefs.onChange(() => {
    if (!preview || !preview.el.isConnected) return;
    const next = layout();
    if (JSON.stringify(next) === JSON.stringify(preview.L)) return;
    preview.L = next;
    draw(preview.el, next, true);
  });

  IP.register("gp-layouts", (el) => {
    const where = el.dataset.where === "away" ? "away" : "session";
    function paint() {
      if (!el.isConnected) return;
      const p = W.gpadPrefs.get();
      const cur = where === "away" ? p.away : p.layout;
      const cards = W.gpadPrefs.list().map((it) => {
        const b = W.gpadVariantCard(it, W.gpadPrefs.layoutOf(it.id), it.id === cur);
        b.addEventListener("click", () => {
          el.querySelectorAll(".gpvcard").forEach((x) => x.setAttribute("aria-checked", String(x === b)));
          W.gpadPrefs.select(where, it.id);
        });
        return b;
      });
      el.replaceChildren(...cards);
      requestAnimationFrame(() => { for (const b of cards) W.gpadThumb(b.querySelector(".gpvthumb"), b._L, "l"); });
    }
    paint();
    let last = JSON.stringify(W.gpadPrefs.list()) + W.gpadPrefs.get()[where === "away" ? "away" : "layout"];
    W.gpadPrefs.onChange(() => {
      const now = JSON.stringify(W.gpadPrefs.list()) + W.gpadPrefs.get()[where === "away" ? "away" : "layout"];
      if (now !== last) { last = now; paint(); }
    });
    return {};
  });

  let deadTimer = 0;
  IP.gamepadSet = (k, v) => {
    const L = layout();
    if (k === "stick" && W.gpadModel.STICKS.includes(v)) L.stick = v;
    else if (k === "dead") L.dead = Math.max(0, Math.min(0.4, +v));
    else return;
    W.gpadPrefs.save("away", L);
    if (k === "dead" && preview && preview.el.isConnected) {
      for (const e of preview.el._gp.values()) if (e._c.kind === "stick") e.classList.add("showdead");
      clearTimeout(deadTimer);
      deadTimer = setTimeout(() => { if (preview) for (const e of preview.el._gp.values()) e.classList.remove("showdead"); }, 1200);
    }
  };
})();
