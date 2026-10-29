// wado bridge — the trackpad's rail and quick bar: what sits beside the pad besides the pad.
//
// Landscape: a rail down the left of the pad, as this device's `rail` setting says —
//   keys     modifiers (Super, Ctrl, Alt, Shift), Esc and Tab, the arrows, and Type
//   actions  the `quick` actions from the computer's list, in the user's order, then Type
//   none     nothing: the pad is the whole screen
// Portrait: the keys already live with the keyboard (layout A), so only `actions` shows, as a
// bar above the pad.
//
// A quick action is pressed, then runs: a short spinner, then a check or a shake. The error's
// words go to the alert card (`onError`). Nothing here touches the touch path.
//
//   W.padRail.mount({rail, bar})   the two containers (pad_ui builds them)
//   W.padRail.paint()              rebuild from the settings, the action list and the layout
//   W.padRail.items(quick, list) → the actions to show: `quick`'s order, ids not in `list` skipped
//   W.padRail.onType(fn), onSettings(fn), onError(fn), onDone(fn(action, {ok, message}))

(() => {
  const R = (W.padRail = {});
  const MODS = [["super", "Super"], ["ctrl", "Ctrl"], ["alt", "Alt"], ["shift", "Shift"]];
  const KEYS = [["esc", "Esc", "Escape"], ["tab", "Tab", "Tab"], ["left", "←", "Left arrow"],
    ["right", "→", "Right arrow"], ["up", "↑", "Up arrow"], ["down", "↓", "Down arrow"]];
  const SPIN_MIN_MS = 110; // long enough to read as "it ran", short enough to keep the total ≤ 400
  const MARK_MS = 280;
  let box = null;
  const hooks = { type: [], settings: [], error: [], done: [] };
  const call = (k, ...a) => { for (const fn of hooks[k]) { try { fn(...a); } catch (_) {} } };
  R.onType = (fn) => hooks.type.push(fn);
  R.onSettings = (fn) => hooks.settings.push(fn);
  R.onError = (fn) => hooks.error.push(fn);
  R.onDone = (fn) => hooks.done.push(fn);

  R.items = (quick, list) => {
    const by = new Map((list || []).map((a) => [a.id, a]));
    const seen = new Set();
    return (quick || []).filter((id) => by.has(id) && !seen.has(id) && seen.add(id)).map((id) => by.get(id));
  };

  function node(tag, cls, attrs) {
    const n = document.createElement(tag);
    if (cls) n.className = cls;
    for (const k in attrs || {}) n.setAttribute(k, attrs[k]);
    return n;
  }
  // Each item enters a beat after the one before it (pad.css: `--i`), so the rail reads top down.
  const stagger = (items, from = 0) => items.forEach((n, i) => n.style.setProperty("--i", String(from + i)));

  function keyButton(attrs, label) {
    const b = node("button", "tpkey", attrs);
    b.textContent = label;
    return b;
  }

  function typeButton() {
    const b = node("button", "tptype", { "aria-label": "Type on the computer" });
    b.innerHTML = W.padIcon("kbd") + "<span>Type</span>";
    b.addEventListener("click", () => call("type"));
    return b;
  }

  function settingsButton() {
    const b = node("button", "tpgear iconbtn", { "aria-label": "Trackpad settings" });
    b.innerHTML = W.padIcon("sliders");
    b.addEventListener("click", () => call("settings"));
    return b;
  }

  function keysRail() {
    const grid = node("div", "tprgrid");
    for (const [k, l] of MODS) grid.appendChild(keyButton({ "data-mod": k, "aria-pressed": "false" }, l));
    for (const [k, l, name] of KEYS) grid.appendChild(keyButton({ "data-key": k, "aria-label": name }, l));
    grid.addEventListener("click", (e) => {
      const b = e.target.closest("button");
      if (!b) return;
      if (b.dataset.mod) W.padKeys.tapMod(b.dataset.mod);
      else if (b.dataset.key) W.padKeys.key(b.dataset.key);
    });
    return grid;
  }

  // What may overflow scrolls on its own; Type and settings stay put below it.
  function scroller(items) {
    const box = node("div", "tprscroll");
    box.append(...items);
    stagger(items);
    return box;
  }

  function actionButton(a, cls) {
    const b = node("button", cls, { "data-action": a.id });
    const ico = node("span", "tpico");
    ico.innerHTML = W.padIcon(a.icon);
    const mark = node("span", "tpmark", { "aria-hidden": "true" });
    mark.innerHTML = `<svg class="i" viewBox="0 0 24 24"><path pathLength="1" d="M20 6 9 17l-5-5"/></svg>`;
    const label = node("span", "tplabel");
    label.textContent = a.name;
    b.append(ico, node("span", "tpspin", { "aria-hidden": "true" }), mark, label);
    b.addEventListener("click", () => run(b, a));
    return b;
  }

  async function run(b, a) {
    const id = a.id;
    if (b.dataset.state === "run") return;
    const t0 = performance.now();
    b.dataset.state = "run";
    b.setAttribute("aria-busy", "true");
    const res = await W.padActions.run(id);
    const wait = SPIN_MIN_MS - (performance.now() - t0);
    if (wait > 0) await new Promise((r) => setTimeout(r, wait));
    b.removeAttribute("aria-busy");
    b.dataset.state = res.ok ? "ok" : "bad";
    call("done", a, res);
    if (!res.ok) call("error", res.message || "That didn't work.");
    setTimeout(() => { if (b.dataset.state !== "run") delete b.dataset.state; }, MARK_MS);
  }

  function note(text) {
    const p = node("p", "tprnote");
    p.textContent = text;
    return p;
  }

  function actionItems(cls) {
    const A = W.padActions;
    if (A.denied) return [note(A.denied)];
    const items = R.items(W.padPrefs.get().quick, A.list);
    if (!items.length) {
      return [note(A.list.length ? "No quick actions yet — pick some in settings." : "Asking the computer what it can do…")];
    }
    return items.map((a) => actionButton(a, cls));
  }

  R.mount = (refs) => { box = refs; };

  R.paint = () => {
    if (!box) return;
    const { rail } = W.padPrefs.get();
    const landscape = W.padOrient.layout === "landscape";
    let side = [];
    if (landscape && rail === "keys") {
      const grid = keysRail();
      stagger([...grid.children]);
      side = [node("div", "tprscroll"), typeButton()];
      side[0].appendChild(grid);
    } else if (landscape && rail === "actions") {
      const items = actionItems("tpact");
      side = [scroller(items), typeButton()];
    }
    if (side.length && hooks.settings.length) side.push(settingsButton());
    // Below the scroller, Type and settings enter last.
    const after = side.slice(1);
    stagger(after, 8);
    box.rail.replaceChildren(...side);
    box.rail.hidden = !side.length;

    const bar = !landscape && rail === "actions" ? actionItems("tpact tpact-bar") : [];
    stagger(bar);
    box.bar.replaceChildren(...bar);
    box.bar.hidden = !bar.length;
    // The modifier buttons just built must show what is already latched or locked.
    if (W.padKeys.repaint) W.padKeys.repaint();
  };
})();
