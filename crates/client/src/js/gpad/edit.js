// wado bridge — the gamepad is its own editor (spec §5.6): tap selects a control and, by default,
// its group; drag moves it 1:1 with grid snap and alignment guides; the corner dot resizes (a
// group spreads around its centre, so its buttons never overlap; never under 44 px); arrows nudge
// by 8 px (the single-pointer path, WCAG 2.5.7); Undo / Redo (60 steps), Mirror, Hide, Reset,
// Save as. Every change is saved as it is made (`gpadScreen.commit`).
//
// One capture-phase listener set on the play area: capture runs before a control's own
// `pointerdown`, so a drag never also presses the button.
//
//   W.gpadEdit.attach() · set(on, {quiet}) · paint() · selIds() · members()
//   W.gpadEdit.undo() · redo() · mirror() · nudge(dx, dy) · setScale(v) · setLook(k, v)
//   W.gpadEdit.change(fn)   snapshot, fn(L), redraw, save — the inspector's one way in

(() => {
  const V = W.gpadVariants, M = W.gpadModel, P = W.gpadPlace;
  const E = (W.gpadEdit = {});
  const G = W.gpadScreen;
  let drag = null;

  E.selIds = () => {
    const S = G.S;
    if (!S || !S.sel) return [];
    const c = S.L.controls.find((x) => x.id === S.sel);
    if (!c) return [];
    return S.together ? M.group(S.L, c.id) : [c.id];
  };
  E.members = () => {
    const o = G.o();
    return E.selIds().map((id) => {
      const c = G.S.L.controls.find((x) => x.id === id);
      return { c, pos: c[o] };
    });
  };
  const groupCentre = (ps) => {
    const xs = ps.map((p) => p[0]), ys = ps.map((p) => p[1]);
    return [(Math.min(...xs) + Math.max(...xs)) / 2, (Math.min(...ys) + Math.max(...ys)) / 2];
  };

  E.change = (fn, opts = {}) => {
    G.snapshot();
    fn(G.S.L);
    if (opts.morph) { const n = G.S.L; G.S.L = JSON.parse(G.S.undo[G.S.undo.length - 1]); G.morphTo(n); }
    else G.draw();
    W.gpadInspector.draw(opts.numbersOnly);
    G.commit();
  };

  E.set = (on, opts = {}) => {
    const S = G.S;
    if (!S) return;
    S.editing = !!on;
    G.root.classList.toggle("editing", S.editing);
    W.gpadInput.releaseAll(G.el.pa);
    if (G.ctl) G.ctl.releaseAll();
    if (!S.editing) {
      S.sel = null;
      W.gpadComfort.toggle(false);
    }
    G.draw();
    W.gpadInspector.draw();
    if (S.editing && !opts.quiet) W.gpadMotion.nudge(G.el.pa._gp || new Map());
  };

  E.select = (id) => {
    G.S.sel = id;
    G.draw();
    W.gpadInspector.draw();
  };

  // Portrait: the pad lifts so the selection clears the inspector sheet (G12).
  function lift() {
    const S = G.S, pa = G.el.pa;
    let up = 0;
    if (S.editing && G.o() === "p" && S.sel && !S.comfort) {
      let b = 0;
      for (const id of E.selIds()) { const e = pa._gp && pa._gp.get(id); if (e) b = Math.max(b, e.offsetTop + e.offsetHeight); }
      const sheetTop = innerHeight * 0.54 - 16;
      up = Math.max(0, Math.min(innerHeight * 0.36, b + pa.offsetTop + 20 - sheetTop));
    }
    if (pa._lift === up) return;
    const from = pa._lift || 0;
    pa._lift = up;
    pa.style.translate = up ? `0 ${-up}px` : "";
    if (W.gpadMotion.level() !== "off" && pa.animate) {
      pa.animate([{ translate: `0 ${-from}px` }, { translate: `0 ${-up}px` }], { duration: W.gpadMotion.ms("--dur-sheet", 480), easing: "cubic-bezier(.32,.72,0,1)" });
    }
  }

  E.paint = () => {
    const S = G.S, el = G.el;
    if (!S) return;
    const ids = E.selIds();
    for (const [id, e] of el.pa._gp || []) e.classList.toggle("sel", S.editing && ids.includes(id));
    lift();
    el.undo.disabled = !S.undo.length;
    el.redo.disabled = !S.redo.length;
    if (!S.editing || !ids.length) { el.selbox.classList.remove("on"); return; }
    const pr = G.paRect();
    let l = 1e9, t = 1e9, r = -1e9, b = -1e9;
    for (const id of ids) {
      const e = el.pa._gp.get(id);
      if (!e) continue;
      const q = e.getBoundingClientRect();
      l = Math.min(l, q.left); t = Math.min(t, q.top); r = Math.max(r, q.right); b = Math.max(b, q.bottom);
    }
    const pad = 8;
    el.selbox.style.cssText = `left:${l - pr.left - pad}px;top:${t - pr.top - pad}px;width:${r - l + pad * 2}px;height:${b - t + pad * 2}px`;
    const c = S.L.controls.find((x) => x.id === S.sel);
    const [w, h] = P.size(c, G.o(), 1);
    el.size.textContent = `${(c[G.o()][2] || 1).toFixed(2)}× · ${Math.round(Math.min(w, h))} px`;
    el.selbox.classList.add("on");
  };

  // ── single-pointer paths ──────────────────────────────────────────────────────────────────
  E.nudge = (dx, dy) => {
    if (!E.selIds().length) return;
    const [DW, DH] = V.REF[G.o()];
    E.change(() => { for (const m of E.members()) { m.pos[0] += (dx * 8) / DW; m.pos[1] += (dy * 8) / DH; M.clampPos(m.c, G.o()); } });
  };
  // The Size slider: the selection grows or shrinks, and a group spreads around its centre with
  // it, as the corner dot does — never under 44 px.
  E.setScale = (v, live) => {
    const apply = () => {
      const ms = E.members(), sel = ms.find((m) => m.c.id === G.S.sel) || ms[0];
      if (!sel) return;
      let f = v / (sel.pos[2] || 1);
      for (const m of ms) f = Math.min(Math.max(f, M.minScale(m.c.kind) / m.pos[2]), V.MAX_SCALE / m.pos[2]);
      const gc = groupCentre(ms.map((m) => m.pos));
      for (const m of ms) {
        m.pos[2] = +(m.pos[2] * f).toFixed(3);
        if (ms.length > 1) { m.pos[0] = gc[0] + (m.pos[0] - gc[0]) * f; m.pos[1] = gc[1] + (m.pos[1] - gc[1]) * f; }
        M.clampPos(m.c, G.o());
      }
    };
    if (live) { apply(); G.draw(); W.gpadInspector.draw(true); G.commit(); return; }
    E.change(apply, { numbersOnly: true });
  };

  E.setLook = (k, v) => {
    const S = G.S;
    const ids = S.scope === "all" ? S.L.controls.map((c) => c.id) : E.selIds();
    E.change((L) => {
      if (S.scope === "all" || !ids.length) { L.look[k] = v; for (const c of L.controls) if (c.look) delete c.look[k]; }
      else for (const m of E.members()) (m.c.look = m.c.look || {})[k] = v;
    });
    W.gpadMotion.pop(ids.map((id) => G.el.pa._gp.get(id)).filter(Boolean));
  };

  E.undo = () => {
    const S = G.S;
    if (!S.undo.length) return;
    S.redo.push(JSON.stringify(S.L));
    G.morphTo(JSON.parse(S.undo.pop()), { dur: W.gpadMotion.ms("--dur", 220) + 120 });
    W.gpadInspector.draw();
    G.commit();
  };
  E.redo = () => {
    const S = G.S;
    if (!S.redo.length) return;
    S.undo.push(JSON.stringify(S.L));
    G.morphTo(JSON.parse(S.redo.pop()), { dur: W.gpadMotion.ms("--dur", 220) + 120 });
    W.gpadInspector.draw();
    G.commit();
  };
  E.mirror = () => {
    G.snapshot();
    G.morphTo(M.mirror(G.S.L));
    W.gpadInspector.draw();
    G.commit();
    G.say("Mirrored for the left hand");
  };

  // ── drag, resize, guides ──────────────────────────────────────────────────────────────────
  function down(e) {
    const S = G.S;
    if (!S || !S.editing) return;
    if (e.target.closest(".gphandle")) return startResize(e);
    const ctl = e.target.closest(".gpc");
    if (!ctl) {
      if (e.target === G.el.pa) { S.sel = null; G.draw(); W.gpadInspector.draw(); }
      return;
    }
    e.preventDefault();
    e.stopPropagation();
    const id = ctl.dataset.id;
    if (!E.selIds().includes(id) || !S.together) { S.sel = id; S.tab = S.tab || "place"; G.draw(); W.gpadInspector.draw(); }
    drag = { x: e.clientX, y: e.clientY, start: E.members().map((m) => [...m.pos]), snap: JSON.stringify(S.L), moved: false };
    try { G.el.pa.setPointerCapture(e.pointerId); } catch (_) {}
  }

  function move(e) {
    if (!drag) return;
    e.preventDefault();
    e.stopPropagation();
    if (drag.resize) return resizeMove(e);
    const S = G.S, o = G.o(), [DW, DH] = V.REF[o];
    if (!drag.moved && Math.hypot(e.clientX - drag.x, e.clientY - drag.y) < 4) return;
    drag.moved = true;
    G.root.classList.add("dragging");
    let dx = (e.clientX - drag.x) / DW, dy = (e.clientY - drag.y) / DH;
    const ms = E.members();
    const c0 = groupCentre(drag.start);
    let nx = c0[0] + dx, ny = c0[1] + dy;
    if (S.snap) { nx = Math.round(nx * 40) / 40; ny = Math.round(ny * 20) / 20; }
    // Guides: the screen's centre line, and every other group's centre.
    const mine = new Set(ms.map((m) => m.c.id));
    const xs = [0.5], ys = [];
    for (const c of S.L.controls) { if (c.hidden || mine.has(c.id)) continue; xs.push(c[o][0]); ys.push(c[o][1]); }
    let gx = null, gy = null;
    for (const x of xs) if (Math.abs(c0[0] + dx - x) < 0.012) { nx = x; gx = x; break; }
    for (const y of ys) if (Math.abs(c0[1] + dy - y) < 0.012) { ny = y; gy = y; break; }
    dx = nx - c0[0]; dy = ny - c0[1];
    ms.forEach((m, i) => { m.pos[0] = drag.start[i][0] + dx; m.pos[1] = drag.start[i][1] + dy; M.clampPos(m.c, o); });
    const el = G.el;
    el.gv.classList.toggle("on", gx != null);
    if (gx != null) el.gv.style.left = P.anchor(gx, DW, el.pa.clientWidth, 1) + "px";
    el.gh.classList.toggle("on", gy != null);
    if (gy != null) el.gh.style.top = P.anchor(gy, DH, el.pa.clientHeight, 1) + "px";
    G.draw();
  }

  function up() {
    if (!drag) return;
    const S = G.S;
    if (drag.moved || drag.resized) {
      if (drag.moved) clearHud();
      S.undo.push(drag.snap);
      if (S.undo.length > 60) S.undo.shift();
      S.redo = [];
      if (navigator.vibrate && W.gpadPrefs.get().haptics !== "off") { try { navigator.vibrate(6); } catch (_) {} }
      G.commit();
    }
    drag = null;
    G.root.classList.remove("dragging");
    G.el.gv.classList.remove("on");
    G.el.gh.classList.remove("on");
    G.draw();
    W.gpadInspector.draw();
  }

  // The HUD's rectangle is reserved: a control dropped under it is moved down clear of it.
  function clearHud() {
    const bar = G.el.ebar.getBoundingClientRect();
    if (!bar.height) return;
    const o = G.o(), [, DH] = V.REF[o];
    for (const m of E.members()) {
      const e = G.el.pa._gp.get(m.c.id);
      if (!e) continue;
      const q = e.getBoundingClientRect();
      const hit = q.top < bar.bottom && q.bottom > bar.top && q.right > bar.left && q.left < bar.right;
      if (hit) { m.pos[1] += (bar.bottom + 6 - q.top) / DH; M.clampPos(m.c, o); }
    }
  }

  function startResize(e) {
    e.preventDefault();
    e.stopPropagation();
    const box = G.el.selbox.getBoundingClientRect();
    const cx = box.left + box.width / 2, cy = box.top + box.height / 2;
    const start = E.members().map((m) => [...m.pos]);
    drag = { resize: true, cx, cy, d0: Math.hypot(e.clientX - cx, e.clientY - cy), start, gc: groupCentre(start), snap: JSON.stringify(G.S.L) };
    try { G.el.pa.setPointerCapture(e.pointerId); } catch (_) {}
  }
  function resizeMove(e) {
    const ms = E.members(), o = G.o();
    let f = Math.hypot(e.clientX - drag.cx, e.clientY - drag.cy) / Math.max(20, drag.d0);
    // One clamp for the whole group, or a member that hits the floor first is left behind.
    ms.forEach((m, i) => {
      f = Math.max(f, M.minScale(m.c.kind) / (drag.start[i][2] || 1));
      f = Math.min(f, V.MAX_SCALE / (drag.start[i][2] || 1));
    });
    if (G.S.snap) f = Math.round(f * 20) / 20 || f;
    ms.forEach((m, i) => {
      const s0 = drag.start[i];
      m.pos[2] = Math.max(M.minScale(m.c.kind), +(s0[2] * f).toFixed(3));
      if (ms.length > 1) { m.pos[0] = drag.gc[0] + (s0[0] - drag.gc[0]) * f; m.pos[1] = drag.gc[1] + (s0[1] - drag.gc[1]) * f; }
      M.clampPos(m.c, o);
    });
    drag.resized = true;
    G.draw();
    W.gpadInspector.draw(true);
  }

  E.attach = () => {
    const pa = G.el.pa;
    pa.addEventListener("pointerdown", down, true);
    pa.addEventListener("pointermove", move, true);
    pa.addEventListener("pointerup", up, true);
    pa.addEventListener("pointercancel", up, true);
    document.addEventListener("keydown", (e) => {
      const S = G.S;
      if (!S || !S.editing || G.root.hidden) return;
      if (e.target && (e.target.tagName === "INPUT" || e.target.tagName === "SELECT")) return;
      const d = { ArrowUp: [0, -1], ArrowDown: [0, 1], ArrowLeft: [-1, 0], ArrowRight: [1, 0] }[e.key];
      if (d && S.sel) { e.preventDefault(); E.nudge(d[0], d[1]); }
      if ((e.key === "z" || e.key === "Z") && (e.ctrlKey || e.metaKey)) { e.preventDefault(); if (e.shiftKey) E.redo(); else E.undo(); }
    });
  };

  // For tests: a drag in reference px, as the pointer would make it.
  E._drag = { down, move, up };
})();
