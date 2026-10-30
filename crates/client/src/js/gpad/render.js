// wado bridge — the gamepad's one renderer. Every surface draws from it: the sessionless play
// screen and its editor, the in-session overlay, the console deck, the picker's thumbnails, the hub
// and settings previews.
//
// One element per control id, reused across renders, so a variant swap can FLIP from the same node
// (`gpad/motion.js`) and a held press survives a re-layout. Classes are namespaced `gp*`; what a
// control looks like is `data-*` on it (kind, shape, style, labels, press), so the CSS
// (`assets/gpad.css`) never collides with the app's generic class names.
//
// Colours are base16 slots by position (A base0B, B base08, X base0D, Y base0A; the shapes family
// ✕ base0D ○ base08 □ base0E △ base0C), always with a letter or a shape beside them (§8).
//
//   W.gpadRender(box, L, {o, k, place?, live?, deck?}) → Map id → element
//     place: a Map id → {x,y,w,h} to use instead of `gpadPlace.all` (the deck packs its own)
//     live:  the controls are real (roles, labels); otherwise a picture (aria-hidden)
//   W.gpadIcon(name)   the small set of glyphs the pad draws

(() => {
  const V = W.gpadVariants, M = W.gpadModel, P = W.gpadPlace;
  const XCOL = { a: "--base0B", b: "--base08", x: "--base0D", y: "--base0A" };
  const SCOL = { a: "--base0D", b: "--base08", x: "--base0E", y: "--base0C" };
  // ✕ ○ □ △ drawn as plain SVG, never a brand glyph font.
  const SHAPE = {
    a: '<path d="M6 6l12 12M18 6L6 18"/>', b: '<circle cx="12" cy="12" r="7"/>',
    x: '<rect x="5.5" y="5.5" width="13" height="13" rx="1"/>', y: '<path d="M12 4.5l8 14H4z"/>',
  };
  const ARROW = { u: '<path d="M12 5l7 10H5z"/>', d: '<path d="M12 19l7-10H5z"/>', l: '<path d="M5 12l10 7V5z"/>', r: '<path d="M19 12L9 19V5z"/>' };
  const GLYPH = {
    view: '<rect x="4" y="5" width="9" height="9" rx="1.5"/><rect x="11" y="10" width="9" height="9" rx="1.5"/>',
    menu: '<path d="M5 7h14M5 12h14M5 17h14"/>', home: '<path d="M4 11l8-7 8 7M6 10v10h12V10"/>',
    up: '<path d="M12 19V5M6 11l6-6 6 6"/>',
    // The editor's chrome (the app's own set, `pad_icons.js`, has the rest).
    undo: '<path d="M9 14L4 9l5-5"/><path d="M4 9h10a6 6 0 010 12h-3"/>',
    redo: '<path d="M15 14l5-5-5-5"/><path d="M20 9H10a6 6 0 000 12h3"/>',
    mirror: '<path d="M12 3v18"/><path d="M8 7l-5 5 5 5zM16 7l5 5-5 5z"/>',
    grid: '<path d="M4 9h16M4 15h16M9 4v16M15 4v16"/>',
    layout: '<rect x="3" y="3" width="7.5" height="7.5" rx="2"/><rect x="13.5" y="3" width="7.5" height="7.5" rx="2"/><rect x="3" y="13.5" width="7.5" height="7.5" rx="2"/><rect x="13.5" y="13.5" width="7.5" height="7.5" rx="2"/>',
    reset: '<path d="M4 12a8 8 0 108-8H8M8 1v6h6" transform="translate(0 1)"/>',
  };
  const svg = (paths, cls) => `<svg class="${cls || "gpi"}" viewBox="0 0 24 24" aria-hidden="true">${paths}</svg>`;
  W.gpadIcon = (n) => svg(GLYPH[n] || "", "i");
  W.gpadShape = (bind) => (SHAPE[bind] ? svg(SHAPE[bind]) : "");

  const colour = (c, lk) => {
    if (c.kind !== "btn" || lk.labels === "letters") return "var(--bright)";
    const map = lk.labels === "shapes" ? SCOL : XCOL;
    return map[c.bind] ? `var(${map[c.bind]})` : "var(--bright)";
  };
  W.gpadColour = colour;

  function inner(c, lk) {
    const halo = '<span class="gphalo"></span>';
    switch (c.kind) {
      case "btn":
        if (lk.labels === "shapes" && SHAPE[c.bind]) return `<span class="gpface">${svg(SHAPE[c.bind])}</span>${halo}`;
        return `<span class="gpface">${esc(c.label)}</span>${halo}`;
      case "small":
        return `<span class="gpface">${c.word ? `<span class="gpword">${esc(c.label)}</span>` : svg(GLYPH[c.icon || "menu"])}</span>${halo}`;
      case "bumper":
      case "trig":
        return `<span class="gpface">${esc(c.label)}</span>${halo}`;
      case "pedal":
        return `<span class="gpface"><span class="gpfill"></span>${svg(GLYPH.up, "gpi gpchev")}<span class="gpword">${esc(c.label)}</span></span>${halo}`;
      case "dpad":
        return `<span class="gpface"><span class="gpdp">${["u", "d", "l", "r"].map((d) => `<span class="gparm" data-d="${d}">${svg(ARROW[d])}</span>`).join("")}<span class="gphub"></span></span></span>`;
      case "stick":
        return '<span class="gpface"></span><span class="gpdead"></span><span class="gptrail"></span><span class="gptrail"></span><span class="gptrail"></span>' +
          `<span class="gpdir"></span><span class="gpthumb">${esc(c.label)}</span>`;
      case "slider":
        return '<span class="gpface"></span><span class="gpdir"></span><span class="gpthumb"></span>';
    }
    return "";
  }
  const esc = (s) => String(s || "").replace(/[&<>"]/g, (ch) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[ch]);

  function label(el, c, live) {
    if (!live) { el.setAttribute("aria-hidden", "true"); return; }
    el.removeAttribute("aria-hidden");
    const slider = c.kind === "stick" || c.kind === "pedal" || c.kind === "slider";
    el.setAttribute("role", slider ? "slider" : "button");
    el.setAttribute("aria-label", M.name(c));
    if (slider && !el.hasAttribute("aria-valuetext")) el.setAttribute("aria-valuetext", "centred");
  }

  W.gpadRender = (box, L, opt) => {
    const o = opt.o || "l", k = opt.k || 1;
    const map = box._gp || (box._gp = new Map());
    const place = opt.place || P.all(L, o, k, opt.w != null ? opt.w : box.clientWidth, opt.h != null ? opt.h : box.clientHeight);
    const seen = new Set();
    for (const c of L.controls) {
      const at = place.get(c.id);
      if (c.hidden || !at) continue;
      seen.add(c.id);
      const lk = M.lookOf(c, L);
      let el = map.get(c.id);
      if (!el) {
        el = document.createElement("div");
        el.dataset.id = c.id;
        box.appendChild(el);
        map.set(c.id, el);
      }
      const sig = `${c.kind}|${lk.labels}|${c.label}|${c.icon}|${c.word}`;
      if (el._sig !== sig) { el.innerHTML = inner(c, lk); el._sig = sig; }
      el.classList.add("gpc"); // state classes (on, sel, flag) are other modules' and survive
      const pill = c.kind === "small" || c.kind === "bumper" || c.kind === "slider";
      const d = el.dataset;
      d.kind = c.kind;
      d.shape = pill ? "pill" : c.kind === "trig" ? "round" : lk.shape;
      d.style = lk.style;
      d.labels = lk.labels;
      d.press = lk.press;
      d.dir = String(c.dir || 4);
      d.g = c.g;
      label(el, c, !!opt.live);
      const op = lk.op * (opt.opacity == null ? 1 : opt.opacity);
      el.style.cssText = `left:${at.x}px;top:${at.y}px;width:${at.w}px;height:${at.h}px;--h:${Math.min(at.w, at.h)}px;` +
        `--c:${colour(c, lk)};--op:${op.toFixed(3)};--dz:${L.dead == null ? M.DEAD : L.dead};--jd:${(-((c.id.length * 0.37) % 1.1)).toFixed(2)}s`;
      el._c = c;
    }
    for (const [id, el] of map) if (!seen.has(id)) { el.remove(); map.delete(id); }
    return map;
  };
})();
