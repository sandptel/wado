// wado bridge — the trackpad's own settings sheet, opened from ⚙ in the pad's header (portrait)
// or on the rail (landscape). A bottom sheet upright, a side sheet sideways — it lives inside the
// pad, so it turns with it.
//
// Every change is saved at once through js/pad_prefs.js: the localStorage mirror, then a
// ConfigSet of `device.<this device>.trackpad.<field>`, so it lands in the computer's config and
// follows this device to another browser. Nothing here is applied by this file: the pad's layout
// listens to the settings like it would to a change from the daemon.
//
//   W.padSettings.mount(root)   the pad's root; the sheet is built inside it (again: moved there)
//   W.padSettings.open(from?)   from: the element to give focus back to on close
//   W.padSettings.openHere(from?)  the same sheet with the pad closed — the second door, from a
//                               computer's page (its Trackpad row). One implementation, two
//                               doors: it lives in a screen-wide host of its own until the pad
//                               opens, and goes back inside the pad when it does.
//   W.padSettings.close()       through history when open() pushed an entry, so Back closes it too
//   W.padSettings.hide()        close now (js/pad_ui.js calls it on popstate)
//   W.padSettings.isOpen()

(() => {
  const S = (W.padSettings = {});
  const GESTURES = [
    ["edge-left", "Swipe in from the left edge"],
    ["edge-right", "Swipe in from the right edge"],
    ["four-tap", "Tap with four fingers"],
    ["two-hold", "Hold two fingers still"],
  ];
  const RAILS = [["keys", "Keys"], ["actions", "Quick actions"], ["none", "None"]];
  let root = null;
  let scrim = null;
  let sheet = null;
  let body = null;
  let err = null;
  let back = null;
  let shown = false;
  // The pad's root (the first door), and the screen-wide host the second door opens in.
  let padRoot = null;
  let hostEl = null;
  const host = () => {
    if (!hostEl) {
      hostEl = node("div", "tps-host", { id: "wado-tps-host", "data-layout": "portrait" });
      document.body.appendChild(hostEl);
    }
    return hostEl;
  };
  // Where the sheet belongs right now: in the pad while it is up, else in the host.
  const home = () => (padRoot && !padRoot.hidden ? padRoot : host());
  const rehome = () => {
    const h = home();
    if (sheet && sheet.parentNode !== h) h.append(scrim, sheet);
  };

  function node(tag, cls, attrs, text) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    for (const k in attrs || {}) n.setAttribute(k, attrs[k]);
    if (text != null) n.textContent = text;
    return n;
  }
  // The control moves first, then the change is committed: a commit re-renders the sheet, and a
  // switch or knob rebuilt in its new state would jump there instead of sliding.
  const COMMIT_MS = 200;
  const set = (field, value) => W.padPrefs.set(field, value);
  const later = (fn) => setTimeout(fn, COMMIT_MS);

  // ── controls ───────────────────────────────────────────────────────────────────────────
  function switchRow(title, sub, on, flip, opts = {}) {
    const row = node("div", "tps-row");
    const words = node("div", "tps-words");
    const t = node("span", "tps-title", { id: "tps-" + title.replace(/\W/g, "") }, title);
    words.append(t);
    if (sub) words.append(node("span", "tps-sub", null, sub));
    const sw = node("button", "tps-switch", {
      role: "switch", "aria-checked": String(!!on), "aria-labelledby": t.id, "data-key": "sw:" + t.id,
    });
    sw.appendChild(node("i"));
    if (opts.disabled) sw.disabled = true;
    sw.addEventListener("click", () => { sw.setAttribute("aria-checked", String(!on)); later(() => flip(!on)); });
    row.append(words, sw);
    return row;
  }

  function segmented(label, items, cur, pick) {
    const wrap = node("div", "tps-seg", { role: "radiogroup", "aria-label": label });
    const i = Math.max(0, items.findIndex(([v]) => v === cur));
    wrap.style.setProperty("--i", String(i));
    wrap.style.setProperty("--n", String(items.length));
    wrap.appendChild(node("span", "tps-knob", { "aria-hidden": "true" }));
    for (const [v, l] of items) {
      const b = node("button", "", { role: "radio", "aria-checked": String(v === cur), "data-v": v, "data-key": "seg:" + v }, l);
      b.addEventListener("click", () => {
        wrap.style.setProperty("--i", String(items.findIndex(([x]) => x === v)));
        for (const o of wrap.querySelectorAll("[role=radio]")) o.setAttribute("aria-checked", String(o === b));
        later(() => pick(v));
      });
      wrap.appendChild(b);
    }
    return wrap;
  }

  const section = (title, ...kids) => {
    const s = node("section", "tps-sec");
    s.append(node("h3", "tps-label", null, title), ...kids);
    return s;
  };

  // ── quick actions: the chosen ones in order (↑/↓), then everything else by group ─────────
  function quickSection(p) {
    const A = W.padActions;
    if (A.denied) return section("Quick actions", node("p", "tps-note", null, A.denied));
    if (!A.list.length) return section("Quick actions", node("p", "tps-note", null, "Asking the computer what it can do…"));
    const chosen = W.padRail.items(p.quick, A.list);
    const list = node("div", "tps-list");
    const move = (id, by) => {
      const q = chosen.map((a) => a.id);
      const i = q.indexOf(id), j = i + by;
      if (i < 0 || j < 0 || j >= q.length) return;
      [q[i], q[j]] = [q[j], q[i]];
      set("quick", q);
    };
    const toggle = (id, on) => {
      const q = chosen.map((a) => a.id).filter((x) => x !== id);
      if (on) q.push(id);
      set("quick", q);
    };
    const row = (a, on, i) => {
      const r = node("div", "tps-row tps-act", { "data-id": a.id });
      const ico = node("span", "tps-ico");
      ico.innerHTML = W.padIcon(a.icon);
      const t = node("span", "tps-title", { id: "tpsa-" + a.id }, a.name);
      const words = node("div", "tps-words");
      words.append(t);
      r.append(ico, words);
      if (on) {
        for (const [by, label, glyph] of [[-1, "Move up", "up"], [1, "Move down", "down"]]) {
          const b = node("button", "tps-move iconbtn", { "aria-label": `${label}: ${a.name}`, "data-move": String(by),
            "data-key": `mv:${a.id}:${by}` });
          b.innerHTML = W.padIcon(glyph);
          if ((by < 0 && i === 0) || (by > 0 && i === chosen.length - 1)) b.disabled = true;
          b.addEventListener("click", () => move(a.id, by));
          r.appendChild(b);
        }
      }
      const sw = node("button", "tps-switch", { role: "switch", "aria-checked": String(on), "aria-labelledby": t.id,
        "data-key": "act:" + a.id });
      sw.appendChild(node("i"));
      sw.addEventListener("click", () => { sw.setAttribute("aria-checked", String(!on)); later(() => toggle(a.id, !on)); });
      r.appendChild(sw);
      return r;
    };
    const kids = [];
    if (chosen.length) {
      const top = node("div", "tps-group tps-chosen");
      top.append(node("h4", "tps-gname", null, p.rail === "actions" ? "On the rail, in this order" : "Chosen, in this order"));
      chosen.forEach((a, i) => top.appendChild(row(a, true, i)));
      kids.push(top);
    }
    const groups = new Map();
    for (const a of A.list) {
      if (chosen.includes(a)) continue;
      if (!groups.has(a.group)) groups.set(a.group, []);
      groups.get(a.group).push(a);
    }
    for (const [g, acts] of groups) {
      const grp = node("div", "tps-group");
      grp.append(node("h4", "tps-gname", null, g));
      acts.forEach((a) => grp.appendChild(row(a, false, 0)));
      kids.push(grp);
    }
    list.append(...kids);
    return section("Quick actions", list);
  }

  function gestureSection(p) {
    const A = W.padActions;
    const intro = node("p", "tps-note", null,
      "Off unless you pick one. Your computer's own touchpad gestures always come first.");
    if (A.denied || !A.list.length) return section("Phone gestures", intro);
    const rows = GESTURES.map(([g, words]) => {
      const r = node("div", "tps-row tps-gest");
      const id = "tpsg-" + g;
      r.appendChild(node("label", "tps-title", { for: id }, words));
      const sel = node("select", "tps-pick", { id, "data-key": id });
      const off = node("option", "", { value: "" }, "Off");
      sel.appendChild(off);
      const groups = new Map();
      for (const a of A.list) {
        if (!groups.has(a.group)) {
          const og = node("optgroup", "", { label: a.group });
          groups.set(a.group, og);
          sel.appendChild(og);
        }
        const o = node("option", "", { value: a.id }, a.name);
        groups.get(a.group).appendChild(o);
      }
      sel.value = p.gestures[g] || "";
      sel.addEventListener("change", () => W.padPrefs.setGesture(g, sel.value));
      r.appendChild(sel);
      return r;
    });
    return section("Phone gestures", intro, ...rows);
  }

  // ── the sheet ──────────────────────────────────────────────────────────────────────────
  function render() {
    if (!body) return;
    const p = W.padPrefs.get();
    // Rows that survive a re-render slide from where they were (an ↑/↓ reorder reads as a move).
    const before = new Map();
    for (const r of body.querySelectorAll("[data-id]")) {
      if (r.getBoundingClientRect) before.set(r.dataset.id, r.getBoundingClientRect().top);
    }
    const scroll = body.scrollTop;
    // The control just used is rebuilt: give focus back to its replacement.
    const ae = document.activeElement;
    const focusKey = ae && ae.dataset && body.contains && body.contains(ae) ? ae.dataset.key : null;
    const secs = [
      section("Layout",
        switchRow("Landscape", "Hold the phone sideways; the pad turns itself", p.landscape, (v) => set("landscape", v)),
        switchRow("Full screen", "Also lock the screen sideways, where the browser allows it", p.fullscreen,
          (v) => set("fullscreen", v), { disabled: !p.landscape }),
        switchRow("Flip", "Turn the sideways pad the other way round", p.flip, (v) => set("flip", v), { disabled: !p.landscape })),
      section("Beside the pad", segmented("Beside the pad", RAILS, p.rail, (v) => set("rail", v))),
      quickSection(p),
      gestureSection(p),
      section("Keyboard",
        switchRow("Keyboard opens with the pad", "", p.keyboard, (v) => set("keyboard", v))),
    ];
    secs.forEach((s, i) => s.style.setProperty("--i", String(i)));
    body.replaceChildren(...secs);
    body.scrollTop = scroll;
    if (focusKey) {
      const again = body.querySelector(`[data-key="${focusKey}"]`) ||
        (focusKey.startsWith("mv:") && body.querySelector(`[data-key="act:${focusKey.split(":")[1]}"]`));
      if (again && again.focus) again.focus({ preventScroll: true });
    }
    for (const r of body.querySelectorAll("[data-id]")) {
      const was = before.get(r.dataset.id);
      if (was == null || !r.animate) continue;
      const dy = was - r.getBoundingClientRect().top;
      if (Math.abs(dy) > 1) {
        r.animate([{ transform: `translateY(${dy}px)` }, { transform: "none" }],
          { duration: 320, easing: "cubic-bezier(.32, .72, 0, 1)" });
      }
    }
  }

  function onKey(e) { if (e.key === "Escape" && shown) { e.preventDefault(); S.close(); } }

  S.mount = (r) => {
    if (r !== hostEl) padRoot = r;
    // Built once; a second mount only moves it.
    if (sheet) { root = r; r.append(scrim, sheet); return; }
    root = r;
    scrim = node("div", "tpscrim", { hidden: "" });
    scrim.addEventListener("click", () => S.close());
    sheet = node("div", "tpsheet", { role: "dialog", "aria-modal": "true", "aria-labelledby": "tps-head", hidden: "" });
    const head = node("header", "tps-head");
    const h = node("h2", "", { id: "tps-head" }, "Trackpad settings");
    const x = node("button", "iconbtn tps-close", { "aria-label": "Close settings" });
    x.innerHTML = W.padIcon("x");
    x.addEventListener("click", () => S.close());
    head.append(h, x);
    err = node("div", "card alertcard tps-err", { role: "alert", hidden: "" });
    body = node("div", "tps-body");
    sheet.append(head, err, body);
    root.append(scrim, sheet);
    W.padPrefs.onChange(() => { if (shown) render(); });
    W.padPrefs.onReject((field, message) => {
      if (!shown) return;
      err.textContent = message || `Couldn't save ${field}.`;
      err.hidden = false;
    });
    W.padActions.onChange(() => { if (shown) render(); });
  };

  S.isOpen = () => shown;

  S.open = (from) => {
    if (!sheet || shown) return;
    rehome();
    shown = true;
    back = from || null;
    err.hidden = true;
    render();
    // The sections cascade in on open only, not on every change after.
    body.classList.add("enter");
    setTimeout(() => body.classList.remove("enter"), 700);
    scrim.hidden = false;
    sheet.hidden = false;
    sheet.classList.remove("out");
    scrim.classList.remove("out");
    document.addEventListener("keydown", onKey);
    // Back closes the sheet, not the whole pad.
    if (typeof history !== "undefined") history.pushState({ wadoPad: true, wadoPadSettings: true }, "");
    const x = sheet.querySelector(".tps-close");
    if (x && x.focus) x.focus({ preventScroll: true });
  };

  S.openHere = (from) => {
    if (!sheet) S.mount(host());
    // With the pad closed nothing has asked for the actions over its link: ask on the relay.
    if (!W.padActions.list.length && !W.padActions.denied) {
      if (W.pad && W.pad.state !== "off") W.padActions.refresh();
      else if (W.actionsGet) W.actionsGet();
    }
    S.open(from);
  };
  // Back closes the sheet opened from a computer's page (the pad's own popstate handles its own).
  addEventListener("popstate", () => { if (shown && hostEl && sheet && sheet.parentNode === hostEl) S.hide(); });

  S.close = () => {
    if (!sheet || !shown) return;
    if (typeof history !== "undefined" && history.state && history.state.wadoPadSettings) { history.back(); return; }
    S.hide();
  };

  S.hide = () => {
    if (!sheet || !shown) return;
    shown = false;
    document.removeEventListener("keydown", onKey);
    sheet.classList.add("out");
    scrim.classList.add("out");
    setTimeout(() => { if (!shown) { sheet.hidden = true; scrim.hidden = true; } }, 240);
    if (back && back.focus) back.focus({ preventScroll: true });
  };
})();
