// wado bridge — the Gamepad screen (spec §5.5): the pad full screen on the stage ground, with no
// session — this phone as a real controller on the computer, over the pad link. The same screen is
// the layout editor (`gpad/edit.js`, `gpad/inspector.js`), its picker (`gpad/picker.js`) and its
// comfort check (`gpad/comfort.js`); opened from a session to edit the session's layout, it sends
// nothing and Done returns to the session.
//
// Owns `#wado-gpad` outright, like the trackpad owns `#wado-trackpad`: fingers move it far too
// often to re-render the app. Dioxus only calls `W.gpadOpen`.
//
//   W.gpadOpen({state?: "play"|"edit"|"comfort"|"picker", where?: "away"|"session", name?})
//   W.gpadClose()
//   W.gpadScreen   the shared state and helpers the editor modules use:
//     .S {where, L, editing, sel, together, snap, undo, redo, tab, scope, comfort, hands}
//     .el · .draw() · .morphTo(L, opts) · .snapshot() · .commit() · .say(text) · .o() · .ctl

(() => {
  const P = W.gpadPlace, M = W.gpadModel;
  const G = (W.gpadScreen = { S: null, el: {} });
  const ic = (n) => (["undo", "redo", "mirror", "grid", "layout", "reset"].includes(n) ? W.gpadIcon(n) : W.padIcon(n));
  let root = null;
  let sender = null;
  let saveTimer = 0;
  const el = G.el;

  G.o = () => (innerWidth >= innerHeight ? "l" : "p");
  G.paRect = () => el.pa.getBoundingClientRect();

  function mount() {
    if (root) return;
    root = document.createElement("div");
    root.id = "wado-gpad";
    root.hidden = true;
    root.setAttribute("role", "dialog");
    root.setAttribute("aria-label", "Gamepad");
    root.innerHTML = `
      <div class="gpwash" aria-hidden="true"></div>
      <div class="gpgrid" aria-hidden="true"></div>
      <div class="gphud" role="toolbar" aria-label="Gamepad">
        <div class="gppill"><button class="iconbtn gpclose" aria-label="Close the gamepad">${ic("x")}</button>
          <span class="gplbl"><span class="gpdot" aria-hidden="true"></span><span class="gpname"></span><span class="gpnum" title="Player">P1</span></span></div>
        <div class="gppill"><button class="iconbtn gppick" aria-label="Layouts">${ic("layout")}</button>
          <button class="iconbtn gpedit" aria-label="Edit the layout">${ic("edit")}</button>
          <button class="iconbtn gpsettings" aria-label="Gamepad settings">${ic("sliders")}</button></div>
      </div>
      <section class="gpstatus" aria-label="What the computer sees">
        <h3 class="gpsname"></h3><p class="gpsub"></p>
        <div class="gpsees"><small>Sees</small><span class="gprd"></span></div>
        <p class="gpaxes mono" aria-hidden="true">L 0.00 0.00 · R 0.00 0.00</p>
      </section>
      <div class="gpebar" role="toolbar" aria-label="Edit the layout">
        <button class="iconbtn gpundo" aria-label="Undo">${ic("undo")}</button>
        <button class="iconbtn gpredo" aria-label="Redo">${ic("redo")}</button>
        <span class="gpsepv" aria-hidden="true"></span><span class="gpename"></span><span class="gpsepv" aria-hidden="true"></span>
        <button class="iconbtn gpcomf" aria-label="Comfort check" aria-pressed="false">${ic("hand")}</button>
        <button class="iconbtn gpmirror" aria-label="Mirror for the left hand">${ic("mirror")}</button>
        <button class="iconbtn gpsnap" aria-label="Snap to grid" aria-pressed="true">${ic("grid")}</button>
        <button class="gpdone">Done</button>
      </div>
      <div class="gppa" role="application">
        <span class="gpguide gpgv" aria-hidden="true"></span><span class="gpguide gpgh" aria-hidden="true"></span>
        <div class="gpselbox" aria-hidden="true"><span class="gpsize mono"></span><span class="gphandle" aria-label="Resize"></span></div>
      </div>
      <div class="gpcomfort" aria-hidden="true"><svg></svg></div>
      <aside class="gpinsp" aria-label="Control"></aside>
      <aside class="gpccard" aria-label="Comfort check"></aside>
      <div class="gpscrim"></div>
      <aside class="gpsheet" aria-label="Layouts"></aside>
      <div class="card alertcard tperr gperr" role="alert" hidden></div>
      <p class="gplive" aria-live="polite"></p>`;
    document.body.appendChild(root);
    for (const k of ["wash", "grid", "hud", "close", "name", "num", "dot", "pick", "edit", "settings", "status", "sname", "sub", "rd", "axes",
      "ebar", "undo", "redo", "ename", "comf", "mirror", "snap", "done", "pa", "gv", "gh", "selbox", "size", "handle", "comfort",
      "insp", "ccard", "scrim", "sheet", "err", "live"]) {
      el[k] = root.querySelector(".gp" + k);
    }
    G.root = root;
    el.close.onclick = () => close(true);
    el.pick.onclick = () => W.gpadPicker.open();
    el.edit.onclick = () => W.gpadEdit.set(true);
    el.settings.onclick = () => { close(true); emit({ type: "gpadSettings" }); };
    el.done.onclick = () => {
      W.gpadEdit.set(false);
      G.flush();
      G.say(G.S.undo.length ? "Saved for this phone, on the computer" : "No changes");
      G.S.undo = [];
      G.S.redo = [];
      if (G.S.where === "session" && G.S.editOnly) close(true);
    };
    el.undo.onclick = () => W.gpadEdit.undo();
    el.redo.onclick = () => W.gpadEdit.redo();
    el.mirror.onclick = () => W.gpadEdit.mirror();
    el.comf.onclick = () => W.gpadComfort.toggle(!G.S.comfort);
    el.snap.onclick = () => { G.S.snap = !G.S.snap; el.snap.setAttribute("aria-pressed", String(G.S.snap)); };
    W.gpadInput.attach(el.pa, {
      get ctl() { return G.ctl; },
      layout: () => G.S.L,
      haptics: () => W.gpadPrefs.get().haptics,
      editing: () => G.S.editing,
    });
    W.gpadEdit.attach();
    W.pad.onState(() => { if (!root.hidden) { paintLink(); if (W.pad.state === "direct" && sender) sender.resend(); } });
    W.pad.onMsg((m) => {
      if (root.hidden || !sender) return;
      if (m.k === "game") { el.num.textContent = `P${m.player || 1}`; G.player = m.player || 1; paintLink(); }
      if (m.k === "pong") paintLink();
      if (m.k === "err" && m.fatal) refuse(m.message);
    });
    addEventListener("resize", () => { if (!root.hidden) G.draw(); });
    addEventListener("popstate", () => {
      if (root.hidden) return;
      if (W.gpadPicker.isOpen()) W.gpadPicker.close(false);
      else close(false);
    });
    W.gpadPrefs.onChange(() => { if (!root.hidden && !G.S.editing) adoptLayout(); });
  }

  // The layout in use changed elsewhere (another browser, the settings page): morph to it.
  function adoptLayout() {
    const next = W.gpadPrefs.layoutFor(G.S.where);
    if (JSON.stringify(next) !== JSON.stringify(G.S.L)) G.morphTo(next);
  }

  G.say = (text) => {
    el.live.textContent = "";
    el.live.textContent = text;
    el.live.classList.remove("show");
    void el.live.offsetWidth;
    el.live.classList.add("show");
  };

  G.draw = () => {
    if (!root || root.hidden) return;
    const o = G.o();
    root.classList.toggle("portrait", o === "p");
    W.gpadRender(el.pa, G.S.L, { o, k: 1, live: true, w: el.pa.clientWidth, h: el.pa.clientHeight });
    el.ename.innerHTML = "";
    el.ename.textContent = G.S.L.name;
    if (M.dirty(G.S.L)) { const s = document.createElement("small"); s.textContent = " · edited"; el.ename.appendChild(s); }
    const r = G.paRect();
    el.grid.style.cssText = `--gx:${r.width / 40}px;--gy:${r.height / 20}px;left:${r.left}px;top:${r.top}px;width:${r.width}px;height:${r.height}px`;
    W.gpadEdit.paint();
    if (G.S.comfort) W.gpadComfort.draw();
  };

  G.morphTo = (next, opts) => {
    const map = el.pa._gp || new Map();
    const first = W.gpadMotion.rects(map);
    const ghosts = W.gpadMotion.ghosts(el.pa, map, new Set(next.controls.filter((c) => !c.hidden).map((c) => c.id)));
    G.S.L = next;
    G.draw();
    W.gpadMotion.morph(el.pa, first, ghosts, opts);
  };

  G.snapshot = () => {
    const S = G.S;
    S.undo.push(JSON.stringify(S.L));
    if (S.undo.length > 60) S.undo.shift();
    S.redo = [];
  };

  // Every change is saved as it is made (§5.6); Undo is the safety net, not a dialog.
  G.commit = () => {
    G.S.changed = true;
    clearTimeout(saveTimer);
    saveTimer = setTimeout(G.flush, 400);
  };
  G.flush = () => {
    clearTimeout(saveTimer);
    saveTimer = 0;
    if (!G.S || !G.S.changed) return;
    G.S.changed = false;
    const id = W.gpadPrefs.save(G.S.where, G.S.L);
    if (!id) { G.say("Eight layouts is the most — delete one in Layouts to save this."); return; }
    if (G.S.L.id !== id) { G.S.L.id = id; }
  };

  function paintLink() {
    const s = W.pad.state;
    el.dot.dataset.tone = s === "direct" ? "good" : s === "relayed" ? "warn" : "dim";
    const name = G.name || "the computer";
    el.name.textContent = name;
    el.sname.textContent = name;
    const way = s === "direct" ? "Direct" : s === "relayed" ? "Relayed" : "Connecting…";
    const rtt = s !== "off" && W.pad.rtt != null ? ` ${W.pad.rtt} ms` : "";
    el.sub.textContent = G.S.where === "session"
      ? "Arranging the pad you use in a session"
      : `Xbox 360 controller · player ${G.player || 1} · ${way}${rtt}`;
  }

  function readout() {
    const sees = G.ctl.sees();
    el.rd.replaceChildren(...(sees.length ? sees : ["nothing pressed"]).map((t) => {
      const s = document.createElement("span");
      s.textContent = t;
      if (!sees.length) s.className = "none";
      return s;
    }));
    const a = G.ctl.axes;
    el.axes.textContent = `L ${a.lx.toFixed(2)} ${a.ly.toFixed(2)} · R ${a.rx.toFixed(2)} ${a.ry.toFixed(2)}`;
  }

  // The computer said no (no grant, locked, no /dev/uinput): a card that says what to do.
  function refuse(message) {
    if (G.ctl) G.ctl.releaseAll();
    W.gpadInput.releaseAll(el.pa);
    root.classList.add("refused");
    W.padWhyShow(el.err, message, retry, "gamepad");
  }
  function retry() {
    if (!root || root.hidden) return;
    el.err.hidden = true;
    root.classList.remove("refused");
    startLink();
  }
  function startLink() {
    sender = W.gpadSendPad();
    G.ctl.setSender(sender);
    W.pad.start();
    sender.start();
    paintLink();
  }

  // An older daemon cannot take the gamepad away from a session (§5.9): said, not tried.
  const tooOld = () => !!(W.hostConfig && !(W.hostConfig.caps || []).includes("pad-gamepad"));

  function open(opts) {
    mount();
    if (!root.hidden) return;
    const where = opts.where === "session" ? "session" : "away";
    G.name = opts.name || G.name || "";
    G.S = {
      where, editOnly: where === "session", L: W.gpadPrefs.layoutFor(where), editing: false, sel: null, together: true, snap: true,
      undo: [], redo: [], tab: "place", scope: "one", comfort: false, hands: "auto",
    };
    G.ctl = W.gpadController(null);
    G.ctl.onChange(readout);
    root.hidden = false;
    root.classList.toggle("editonly", where === "session");
    root.setAttribute("aria-label", `Gamepad for ${G.name || "the computer"}`);
    el.pa.setAttribute("aria-label", `Gamepad for ${G.name || "the computer"}`);
    el.err.hidden = true;
    root.classList.remove("refused");
    document.documentElement.classList.add("gpad-open");
    history.pushState({ wadoGpad: true }, "");
    G.draw();
    readout();
    paintLink();
    if (where === "away") {
      if (tooOld()) {
        root.classList.add("refused");
        W.padWhyShow(el.err, "older daemon: no gamepad away from a session", null, "gamepad");
      } else startLink();
    }
    const st = opts.state || "play";
    if (st === "edit" || st === "comfort" || where === "session") W.gpadEdit.set(true, { quiet: true });
    if (st === "comfort") W.gpadComfort.toggle(true);
    W.gpadMotion.assemble(el.pa._gp, G.o());
    if (st === "picker") setTimeout(() => W.gpadPicker.open(), 60);
    if (W.wake) W.wake.acquire();
  }

  // `viaHistory`: closed by a button, so take back the entry `open` pushed.
  function close(viaHistory) {
    if (!root || root.hidden) return;
    if (viaHistory && history.state && history.state.wadoGpad) { history.back(); return; }
    G.flush();
    W.gpadPicker.close(true);
    W.gpadComfort.toggle(false);
    W.gpadEdit.set(false, { quiet: true });
    W.gpadInput.releaseAll(el.pa);
    if (G.ctl) G.ctl.releaseAll();
    if (sender) {
      sender.stop();
      sender = null;
      // The trackpad shares the link; it is only ever up while the pad screen is, never both.
      if (!document.documentElement.classList.contains("trackpad-open")) W.pad.stop();
    }
    root.hidden = true;
    document.documentElement.classList.remove("gpad-open");
    if (W.wake && !W.sessionOn) W.wake.release();
    emit({ type: "gpadClosed" });
  }

  W.gpadOpen = (opts) => {
    opts = opts || {};
    // Away from a session the controller is this computer's input: the same passkey step-up as
    // the trackpad (D3). Editing the session's layout needs nothing.
    if (opts.where !== "session" && W.passkeyThen) W.passkeyThen("pad", () => open(opts));
    else open(opts);
  };
  W.gpadClose = () => close(true);
})();
