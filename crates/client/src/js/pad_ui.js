// wado bridge — the Trackpad screen. Owns `#wado-trackpad` outright (Dioxus only calls `padOpen()`),
// like the file manager owns `#wado-files`: fingers move it 60–120 times a second, which must
// not re-render the app.
//
// Layout A (docs/superpowers/specs/2026-10-29-trackpad-mode-design.md): the whole screen is the
// pad; on top, the link's true state in plain words; below, a Keyboard button that opens the
// modifier row and a text line over the phone's own keyboard, with the pad still live above.
// The wire is js/pad_link.js, touches js/pad_touch.js, keys js/pad_keys.js.

(() => {
  const SVG = (d) => `<svg class="i" viewBox="0 0 24 24" aria-hidden="true"><path d="${d}"/></svg>`;
  const CLOSE = SVG("M6 6l12 12M18 6L6 18");
  const KEYBOARD = SVG("M3 7a2 2 0 0 1 2-2h14a2 2 0 0 1 2 2v10a2 2 0 0 1-2 2H5a2 2 0 0 1-2-2zM7 9h.01M11 9h.01M15 9h.01M7 13h.01M17 13h.01M8 16h8");
  const MODS = [["super", "Super"], ["ctrl", "Ctrl"], ["alt", "Alt"], ["shift", "Shift"]];
  const KEYS = [["esc", "Esc"], ["tab", "Tab"], ["left", "←"], ["up", "↑"], ["down", "↓"], ["right", "→"]];
  const WHY = {
    direct: "Touches go straight to the computer over your network.",
    relayed: "No direct path to the computer — touches go through wado's relay, so expect some delay.",
    off: "Connecting to the computer…",
  };

  let root = null;
  let el = {};
  const fingers = new Map();

  function mount() {
    if (root) return;
    root = document.createElement("div");
    root.id = "wado-trackpad";
    root.hidden = true;
    root.setAttribute("role", "dialog");
    root.setAttribute("aria-label", "Trackpad");
    root.innerHTML = `
      <header class="tphead">
        <button class="linkpill tppill dim" aria-expanded="false"><span class="linkdot"></span><span class="tppilltext">Connecting…</span></button>
        <button class="iconbtn tpclose" aria-label="Close trackpad">${CLOSE}</button>
      </header>
      <p class="tpwhy" hidden></p>
      <div class="tparea">
        <p class="tphint">Two fingers to scroll · your computer's gestures work here</p>
        <div class="card alertcard tperr" role="alert" hidden></div>
      </div>
      <div class="tpkeys" hidden>
        <div class="tprow" role="toolbar" aria-label="Modifiers and keys">
          ${MODS.map(([k, l]) => `<button data-mod="${k}" aria-pressed="false">${l}</button>`).join("")}
          <span class="tpsep" aria-hidden="true"></span>
          ${KEYS.map(([k, l]) => `<button data-key="${k}" aria-label="${k === "esc" ? "Escape" : k === "tab" ? "Tab" : k + " arrow"}">${l}</button>`).join("")}
        </div>
        <textarea class="tptext" rows="1" autocapitalize="sentences" autocomplete="off" autocorrect="on"
          spellcheck="false" enterkeyhint="enter" aria-label="Type on the computer"
          placeholder="Type here — it goes to the computer"></textarea>
      </div>
      <button class="btn tpkb" aria-expanded="false">${KEYBOARD}<span>Keyboard</span></button>`;
    document.body.appendChild(root);
    for (const k of ["pill", "pilltext", "close", "why", "area", "hint", "err", "keys", "row", "text", "kb"]) {
      el[k] = root.querySelector(".tp" + k);
    }

    el.close.onclick = () => close(true);
    el.pill.onclick = () => {
      const open = el.why.hidden;
      el.why.hidden = !open;
      el.pill.setAttribute("aria-expanded", String(open));
    };
    el.err.onclick = () => { el.err.hidden = true; };
    el.kb.onclick = () => keyboard(el.keys.hidden);
    el.row.onclick = (e) => {
      const b = e.target.closest("button");
      if (!b) return;
      if (b.dataset.mod) W.padKeys.tapMod(b.dataset.mod);
      else if (b.dataset.key) W.padKeys.key(b.dataset.key);
      el.text.focus({ preventScroll: true });
    };
    // Keep the field focused (and the phone keyboard up) when a row button is pressed.
    el.row.onpointerdown = (e) => e.preventDefault();

    W.padKeys.onMods((m) => {
      for (const b of el.row.querySelectorAll("[data-mod]")) {
        const st = m[b.dataset.mod];
        b.classList.toggle("latched", st === "latched");
        b.classList.toggle("locked", st === "locked");
        b.setAttribute("aria-pressed", st === "off" ? "false" : "true");
      }
    });
    W.pad.onState(paintPill);
    W.pad.onMsg((m) => {
      if (m.k === "pong") paintPill();
      if (m.k === "err") { el.err.textContent = m.message; el.err.hidden = false; }
    });

    // Finger rings: drawn with transform only, never laid out.
    const ring = (e) => {
      let f = fingers.get(e.pointerId);
      if (!f) {
        f = document.createElement("i");
        f.className = "tpfinger";
        el.area.appendChild(f);
        fingers.set(e.pointerId, f);
      }
      const r = el.area.getBoundingClientRect();
      f.style.transform = `translate(${e.clientX - r.left}px, ${e.clientY - r.top}px)`;
    };
    const unring = (e) => { const f = fingers.get(e.pointerId); if (f) { f.remove(); fingers.delete(e.pointerId); } };
    el.area.addEventListener("pointerdown", (e) => { el.area.classList.add("touched"); ring(e); });
    el.area.addEventListener("pointermove", (e) => { if (fingers.has(e.pointerId)) ring(e); });
    for (const ev of ["pointerup", "pointercancel", "lostpointercapture"]) el.area.addEventListener(ev, unring);

    window.addEventListener("popstate", () => { if (!root.hidden) close(false); });
    if (window.visualViewport) {
      const fit = () => {
        if (root.hidden) return;
        const vv = window.visualViewport;
        root.style.height = vv.height + "px";
        root.style.transform = `translateY(${vv.offsetTop}px)`;
      };
      window.visualViewport.addEventListener("resize", fit);
      window.visualViewport.addEventListener("scroll", fit);
    }
  }

  function paintPill() {
    const s = W.pad.state;
    const ms = W.pad.rtt == null ? "" : ` · ${W.pad.rtt} ms`;
    el.pilltext.textContent = s === "direct" ? "Direct" + ms : s === "relayed" ? "Relayed" + ms : "Connecting…";
    el.pill.classList.toggle("good", s === "direct");
    el.pill.classList.toggle("warn", s === "relayed");
    el.pill.classList.toggle("dim", s === "off");
    el.why.textContent = WHY[s] || WHY.off;
  }

  function keyboard(open) {
    el.keys.hidden = !open;
    el.kb.setAttribute("aria-expanded", String(open));
    el.kb.querySelector("span").textContent = open ? "Hide keyboard" : "Keyboard";
    if (open) el.text.focus();
    else el.text.blur();
  }

  W.padOpen = () => {
    mount();
    if (!root.hidden) return;
    root.hidden = false;
    document.documentElement.classList.add("trackpad-open");
    el.err.hidden = true;
    el.why.hidden = true;
    el.area.classList.remove("touched");
    history.pushState({ wadoPad: true }, "");
    W.pad.start();
    W.padTouch.attach(el.area);
    W.padKeys.attach(el.text, el.row);
    paintPill();
    if (W.wake) W.wake.acquire();
  };

  // `viaHistory`: closed by the button, so take back the entry `padOpen` pushed.
  function close(viaHistory) {
    if (!root || root.hidden) return;
    if (viaHistory && history.state && history.state.wadoPad) { history.back(); return; }
    keyboard(false);
    W.padTouch.detach();
    W.padKeys.detach();
    W.pad.stop();
    for (const f of fingers.values()) f.remove();
    fingers.clear();
    root.hidden = true;
    root.style.height = "";
    root.style.transform = "";
    document.documentElement.classList.remove("trackpad-open");
    if (W.wake && !W.sessionOn) W.wake.release();
  }
  W.padClose = () => close(true);
})();
