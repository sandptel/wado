// wado bridge — the editor's inspector (spec §5.6): a floating card docked on the side away from
// the selection (landscape), a bottom sheet (portrait). Three tabs for the selected control:
//
//   Place   Size (never under 44 px — the slider stops there), See-through, nudge arrows (8 px),
//           Move together, Hide, Reset this control
//   Look    This one / All controls; Shape, Style, Labels, On press — each a picture of itself
//   Behave  per kind: stick Fixed / Floating / Relative + dead zone (the ring drawn live); d-pad
//           4-way / 8-way; steering Sliding / Tilting; a button what it sends; Feel in the hand
//
// With nothing selected: Reset, Layouts, Save as a new layout, On press for all.
// The controls reuse the app's own: `.seg` (cc.css), `.sw`, the thick `.slide`.
//
//   W.gpadInspector.draw(numbersOnly?)

(() => {
  const V = W.gpadVariants, M = W.gpadModel;
  const I = (W.gpadInspector = {});
  const G = W.gpadScreen;
  const E = () => W.gpadEdit;
  const esc = (s) => String(s).replace(/[&<>"]/g, (c) => ({ "&": "&amp;", "<": "&lt;", ">": "&gt;", '"': "&quot;" })[c]);
  const ic = (n) => (["undo", "redo", "mirror", "grid", "layout", "reset"].includes(n) ? W.gpadIcon(n) : W.padIcon(n));

  const seg = (name, items, cur) => {
    const i = Math.max(0, items.findIndex(([v]) => v === cur));
    return `<div class="seg" role="radiogroup" aria-label="${name}" data-seg="${name}" style="--n:${items.length};--i:${i}"><span class="knob"></span>` +
      items.map(([v, l]) => `<button role="radio" data-v="${v}" aria-checked="${v === cur}" class="${v === cur ? "on" : ""}">${l}</button>`).join("") + "</div>";
  };
  const slide = (id, label, v, min, max, step, shown) =>
    `<div class="slide thin gpslide" data-slide="${id}" style="--v:${((v - min) / (max - min)).toFixed(4)}"><span class="fill"></span><span class="thumbway"></span>` +
    `<input type="range" min="${min}" max="${max}" step="${step}" value="${v}" aria-label="${label}" aria-valuetext="${shown}">` +
    `<span class="slidelab">${label}</span><span class="slideval">${shown}</span></div>`;
  const opts = (key, items, cur) => `<div class="gpopts" role="radiogroup" aria-label="${key}">` +
    items.map(([v, l, sym]) => `<button role="radio" data-look="${key}" data-v="${v}" aria-checked="${v === cur}">${sym}${l}</button>`).join("") + "</div>";
  const sym = (cls, style) => `<span class="gpsym ${cls}" style="${style || ""}" aria-hidden="true"></span>`;

  function sendsText(c) {
    if (W.gpadPrefs.get().drives === "keys") {
      const m = W.gpadKeymap && W.gpadKeymap[c.bind];
      return m ? (m.mouse ? `Mouse ${m.mouse}` : `Key ${m.key}`) : "—";
    }
    return `Xbox ${V.NAMES[c.bind] || c.bind.toUpperCase()}`;
  }

  I.draw = (numbersOnly) => {
    const box = G.el.insp, S = G.S;
    if (!S || !S.editing || S.comfort) { box.classList.remove("on"); return; }
    box.classList.add("on");
    const c = S.L.controls.find((x) => x.id === S.sel);
    const o = G.o();
    if (o === "l") {
      const x = c ? c.l[0] : 1;
      box.classList.toggle("left", x > 0.5);
      box.classList.toggle("right", x <= 0.5);
    }
    if (numbersOnly && c) {
      const v = c[o][2];
      const s = box.querySelector('[data-slide="size"]');
      if (s) {
        s.style.setProperty("--v", ((v - 0.6) / 1.6).toFixed(4));
        s.querySelector(".slideval").textContent = `${v.toFixed(2)}×`;
        const inp = s.querySelector("input");
        if (document.activeElement !== inp) inp.value = v;
      }
      return;
    }
    const scroll = box.scrollTop;
    if (!c) {
      box.innerHTML = `<div class="gpih"><span class="gpidisc">${ic("hand")}</span><span><b>Arrange your gamepad</b><small>Tap a control to change it. Drag to move it; the corner dot resizes.</small></span></div>
        <div class="gpgrp"><small>Whole layout</small><div class="gprow2"><button class="btn" data-act="reset">${ic("reset")}Reset</button><button class="btn" data-act="pick">${ic("layout")}Layouts</button></div>
        <div class="gprow2"><button class="btn" data-act="saveas">${ic("copy")}Save as a new layout</button></div></div>
        <div class="gpgrp"><small>On press, for all</small>${opts("press", [["glow", "Glow", sym("s")], ["ring", "Ring", sym("s ring")], ["still", "Still", sym("c")]], S.L.look.press)}</div>
        <p class="gpinfo">${ic("info")}<span>Sizes are kept in real pixels, so a button is the same size under your thumb on a phone and a tablet. Positions follow the screen.</span></p>`;
      wire(box, null);
      return;
    }
    const pos = c[o], lk = M.lookOf(c, S.L), n = E().selIds().length;
    let body = "";
    if (S.tab === "place") {
      body = slide("size", "Size", pos[2], 0.6, 2.2, 0.05, `${pos[2].toFixed(2)}×`) +
        `<small class="gpnote">Never below 44 px: the slider stops there.</small>` +
        slide("op", "See-through", +(1 - lk.op).toFixed(2), 0, 0.7, 0.05, `${Math.round((1 - lk.op) * 100)}%`) +
        `<div class="gpgrp"><small>Move by 8 px</small><div class="gpnudge">` +
        [["0,-1", "up", "Up"], ["-1,0", "left", "Left"], ["0,1", "down", "Down"], ["1,0", "right", "Right"]]
          .map(([d, i, l]) => `<button class="gpn-${i}" data-n="${d}" aria-label="Move ${l.toLowerCase()} 8 px">${ic(i)}</button>`).join("") + `</div></div>` +
        `<div class="switchrow"><span class="swtext"><b>Move together</b><small>${n > 1 ? `${n} controls move as one` : "Alone"}</small></span>` +
        `<button class="sw${S.together ? " on" : ""}" role="switch" aria-checked="${S.together}" aria-label="Move together" data-act="together"></button></div>` +
        `<div class="gprow2"><button class="btn" data-act="hide">${ic("eye")}Hide</button><button class="btn" data-act="resetone">${ic("reset")}Reset</button></div>`;
    } else if (S.tab === "look") {
      body = `<div class="gpgrp">${seg("Change", [["one", n > 1 ? "These" : "This one"], ["all", "All controls"]], S.scope)}</div>` +
        `<div class="gpgrp"><small>Shape</small>${opts("shape", [["circle", "Circle", sym("c")], ["pill", "Pill", sym("p")], ["round", "Rounded", sym("r")]], lk.shape)}</div>` +
        `<div class="gpgrp"><small>Style</small>${opts("style", [["solid", "Solid", sym("c")], ["outline", "Outline", sym("o")], ["flat", "Flat", sym("f")]], lk.style)}</div>` +
        `<div class="gpgrp"><small>Labels</small>${opts("labels", [["letters", "Letters", '<span class="gpglyph">A</span>'], ["shapes", "Shapes", '<span class="gpglyph">△</span>'], ["colors", "Colours", '<span class="gpglyph" style="color:var(--base0B)">A</span>']], lk.labels)}</div>` +
        `<div class="gpgrp"><small>On press</small>${opts("press", [["glow", "Glow", sym("s")], ["ring", "Ring", sym("s ring")], ["still", "Still", sym("c")]], lk.press)}</div>`;
    } else {
      if (c.kind === "stick") {
        const st = S.L.stick || "fixed";
        body = `<div class="gpgrp"><small>Stick</small>${seg("Stick", [["fixed", "Fixed"], ["floating", "Floating"], ["relative", "Relative"]], st)}` +
          `<p class="gpnote">${{ fixed: "Stays where it is drawn.", floating: "Comes to where your thumb lands, then goes home.", relative: "Zero is wherever your thumb lands; nothing moves." }[st]}</p></div>` +
          slide("dead", "Dead zone", S.L.dead, 0, 0.4, 0.01, `${Math.round(S.L.dead * 100)}%`) + `<small class="gpnote">The dashed ring on the stick.</small>`;
      } else if (c.kind === "dpad") {
        body = `<div class="gpgrp"><small>Directions</small>${seg("Directions", [["4", "4-way cross"], ["8", "8-way"]], String(c.dir || 4))}` +
          `<p class="gpnote">8-way reads the corners as two directions at once, which fighting games want.</p></div>`;
      } else if (c.kind === "slider") {
        body = `<div class="gpgrp"><small>Steer by</small>${seg("Steer by", [["slide", "Sliding"], ["tilt", "Tilting the phone"]], c.steer || "slide")}` +
          `<p class="gpnote">Tilting asks the browser for the phone's motion sensor once.</p></div>`;
      } else {
        body = `<div class="gpgrp"><small>Sends</small><span class="gpchip mono">${esc(sendsText(c))}</span>` +
          `<p class="gpnote">A real controller always sends its own button, by position. With "Keys &amp; mouse" in a session, it presses a key.</p></div>`;
      }
      const h = W.gpadPrefs.get().haptics;
      body += `<div class="gpgrp"><small>Feel in the hand</small>${seg("Haptics", [["off", "Off"], ["light", "Light"], ["medium", "Medium"], ["strong", "Strong"]], h)}</div>`;
    }
    const tint = W.gpadColour(c, lk);
    const disc = c.kind === "btn" && lk.labels === "shapes" ? W.gpadShape(c.bind) : esc((c.label || "").slice(0, 2)) || ic(c.kind === "stick" ? "pad" : "layout");
    box.innerHTML = `<div class="gpih"><span class="gpidisc" style="color:${tint}">${disc}</span><span><b>${esc(M.name(c))}</b><small>${n > 1 ? `With ${n - 1} more, moving together` : "On its own"}</small></span></div>` +
      seg("Section", [["place", "Place"], ["look", "Look"], ["behave", "Behave"]], S.tab) + `<div class="gpibody">${body}</div>`;
    box.scrollTop = scroll;
    wire(box, c);
  };

  // The control moves first, then the change commits (a rebuilt knob would jump, not slide).
  const later = (fn) => setTimeout(fn, 160);

  function wire(box, c) {
    const S = G.S;
    box.querySelectorAll(".seg").forEach((sg) => sg.addEventListener("click", (e) => {
      const b = e.target.closest("button[data-v]");
      if (!b) return;
      const v = b.dataset.v, name = sg.dataset.seg;
      sg.style.setProperty("--i", [...sg.querySelectorAll("button")].indexOf(b));
      sg.querySelectorAll("button").forEach((x) => { x.classList.toggle("on", x === b); x.setAttribute("aria-checked", String(x === b)); });
      later(() => {
        if (name === "Section") { S.tab = v; I.draw(); }
        else if (name === "Change") { S.scope = v; I.draw(); }
        else if (name === "Stick") E().change((L) => { L.stick = v; });
        else if (name === "Directions") E().change((L) => { L.controls.find((x) => x.id === c.id).dir = +v; });
        else if (name === "Steer by") E().change((L) => { L.controls.find((x) => x.id === c.id).steer = v; });
        else if (name === "Haptics") { W.gpadPrefs.set("haptics", v); if (navigator.vibrate && v !== "off") try { navigator.vibrate({ light: 6, medium: 10, strong: 18 }[v]); } catch (_) {} }
      });
    }));
    box.querySelectorAll(".gpslide").forEach((s) => {
      const inp = s.querySelector("input"), id = s.dataset.slide;
      const min = +inp.min, max = +inp.max;
      let snapped = false;
      const show = () => {
        s.style.setProperty("--v", ((inp.value - min) / (max - min)).toFixed(4));
        const v = +inp.value;
        s.querySelector(".slideval").textContent = id === "size" ? `${v.toFixed(2)}×` : `${Math.round(v * 100)}%`;
      };
      inp.addEventListener("pointerdown", () => { s.classList.add("grab"); });
      inp.addEventListener("input", () => {
        if (!snapped) { G.snapshot(); snapped = true; }
        s.classList.add("dragging");
        show();
        const v = +inp.value;
        if (id === "size") E().setScale(v, true);
        else if (id === "op") { for (const m of E().members()) (m.c.look = m.c.look || {}).op = +(1 - v).toFixed(2); G.draw(); G.commit(); }
        else if (id === "dead") {
          S.L.dead = v;
          G.draw();
          for (const e of G.el.pa._gp.values()) if (e._c.kind === "stick") e.classList.add("showdead");
          G.commit();
        }
      });
      inp.addEventListener("change", () => {
        snapped = false;
        s.classList.remove("grab", "dragging");
        if (id === "dead") setTimeout(() => { for (const e of G.el.pa._gp.values()) e.classList.remove("showdead"); }, 900);
        I.draw();
      });
    });
    box.querySelectorAll("[data-n]").forEach((b) => b.addEventListener("click", () => { const [x, y] = b.dataset.n.split(",").map(Number); E().nudge(x, y); }));
    box.querySelectorAll("[data-look]").forEach((b) => b.addEventListener("click", () => E().setLook(b.dataset.look, b.dataset.v)));
    const act = (name, fn) => { const b = box.querySelector(`[data-act="${name}"]`); if (b) b.addEventListener("click", fn); };
    act("together", () => { S.together = !S.together; G.draw(); I.draw(); });
    act("hide", () => {
      const ids = E().selIds();
      G.snapshot();
      const next = JSON.parse(JSON.stringify(S.L));
      for (const x of next.controls) if (ids.includes(x.id)) x.hidden = true;
      S.sel = null;
      G.morphTo(next);
      I.draw();
      G.commit();
      G.say("Hidden. Undo, or Reset, brings it back.");
    });
    act("resetone", () => {
      const ids = E().selIds(), base = M.builtin(S.L.base);
      G.snapshot();
      const next = JSON.parse(JSON.stringify(S.L));
      next.controls = next.controls.map((x) => (ids.includes(x.id) ? base.controls.find((b) => b.id === x.id) || x : x));
      G.morphTo(next);
      I.draw();
      G.commit();
    });
    act("reset", () => {
      G.snapshot();
      const base = M.builtin(S.L.base);
      base.id = S.L.id;
      base.name = S.L.name;
      G.morphTo(base);
      I.draw();
      G.commit();
      G.say(`Back to ${V.BUILT[S.L.base].name}. Undo brings your changes back.`);
    });
    act("pick", () => W.gpadPicker.open());
    act("saveas", () => {
      G.flush();
      const id = W.gpadPrefs.saveAs(S.L, null);
      if (!id) { G.say("Eight layouts is the most — delete one in Layouts first."); return; }
      W.gpadPrefs.select(S.where, id);
      S.L = W.gpadPrefs.layoutOf(id);
      if (W.gpadPrefs.get().hand === "left") S.L = M.mirror(S.L);
      S.undo = []; S.redo = [];
      G.draw();
      I.draw();
      G.say(`Saved as "${S.L.name}" on the computer, for this phone`);
    });
  }
})();
