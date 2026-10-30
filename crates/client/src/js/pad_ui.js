// wado bridge — the Trackpad screen. Owns `#wado-trackpad` outright (Dioxus only calls `padOpen()`),
// like the file manager owns `#wado-files`: fingers move it 60–120 times a second, which must
// not re-render the app.
//
// Layout A (docs/superpowers/specs/2026-10-29-trackpad-mode-design.md): the whole screen is the
// pad; on top, the link's true state in plain words; below, a Keyboard button that opens the
// modifier row and a text line over the phone's own keyboard, with the pad still live above.
// Landscape (js/pad_orient.js) puts a rail down the left (js/pad_rail.js): keys, quick actions
// (js/pad_actions.js) or nothing, per this device's settings (js/pad_prefs.js).
// The wire is js/pad_link.js, touches js/pad_touch.js (fed by js/pad_pointer.js, the one owner of
// the pad's pointer listeners), keys js/pad_keys.js.

(() => {
  const CLOSE = W.padIcon("x");
  const KEYBOARD = W.padIcon("kbd");
  const MODS = [["super", "Super"], ["ctrl", "Ctrl"], ["alt", "Alt"], ["shift", "Shift"]];
  const KEYS = [["esc", "Esc"], ["tab", "Tab"], ["left", "←"], ["up", "↑"], ["down", "↓"], ["right", "→"]];
  const WHY = {
    direct: "Touches go straight to the computer over your network.",
    relayed: "No direct path to the computer — touches go through wado's relay, so expect some delay.",
    off: "Connecting to the computer…",
  };

  let root = null;
  let el = {};

  function mount() {
    if (root) return;
    root = document.createElement("div");
    root.id = "wado-trackpad";
    root.hidden = true;
    root.setAttribute("role", "dialog");
    root.setAttribute("aria-label", "Trackpad");
    root.innerHTML = `
      <header class="tphead">
        <button class="linkpill tppill dim" aria-expanded="false"><span class="linkdot"></span><span class="tppilltext">Connecting…</span><span class="tprtt"></span></button>
        <button class="iconbtn tpgearhead" aria-label="Trackpad settings">${W.padIcon("sliders")}</button>
        <button class="iconbtn tpclose" aria-label="Close trackpad">${CLOSE}</button>
      </header>
      <p class="tpwhy" hidden></p>
      <p class="tpnote" role="status" hidden></p>
      <div class="tprail" role="toolbar" aria-label="Keys and quick actions" aria-orientation="vertical" hidden></div>
      <div class="tpbar" role="toolbar" aria-label="Quick actions" hidden></div>
      <div class="tparea">
        <p class="tphint">Two fingers to scroll · your computer's gestures work here</p>
        <div class="card alertcard tperr" role="alert" hidden></div>
      </div>
      <div class="tpkeys" hidden>
        <div class="tprow" role="toolbar" aria-label="Modifiers and keys">
          ${MODS.map(([k, l]) => `<button class="tpkey" data-mod="${k}" aria-pressed="false">${l}</button>`).join("")}
          <span class="tpsep" aria-hidden="true"></span>
          ${KEYS.map(([k, l]) => `<button class="tpkey" data-key="${k}" aria-label="${k === "esc" ? "Escape" : k === "tab" ? "Tab" : k + " arrow"}">${l}</button>`).join("")}
        </div>
        <textarea class="tptext" rows="1" autocapitalize="sentences" autocomplete="off" autocorrect="on"
          spellcheck="false" enterkeyhint="enter" aria-label="Type on the computer"
          placeholder="Type here — it goes to the computer"></textarea>
      </div>
      <button class="btn tpkb" aria-expanded="false">${KEYBOARD}<span>Keyboard</span></button>`;
    document.body.appendChild(root);
    for (const k of ["pill", "pilltext", "rtt", "gearhead", "close", "why", "note", "rail", "bar", "area", "hint", "err", "keys", "row", "text", "kb"]) {
      el[k] = root.querySelector(".tp" + k);
    }

    el.close.onclick = () => close(true);
    el.pill.onclick = () => {
      const open = el.why.hidden;
      el.why.hidden = !open;
      el.pill.setAttribute("aria-expanded", String(open));
    };
    // A plain note goes on a tap; a card with something to do stays until it is done.
    el.err.onclick = (e) => {
      if (e.target.closest("button, code") || el.err.dataset.cause !== "other") return;
      el.err.hidden = true;
    };
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

    W.padRail.mount({ rail: el.rail, bar: el.bar });
    W.padRail.onType(() => keyboard(true));
    W.padRail.onError(alert);
    W.padSettings.mount(root);
    W.padRail.onSettings((b) => W.padSettings.open(b));
    el.gearhead.onclick = () => W.padSettings.open(el.gearhead);
    const ran = (a, res) => {
      W.padConfirm.show(el.area, a, res);
      if (!res.ok) alert(res.message || `${a.name} didn't work.`);
    };
    W.padRail.onDone((a, res) => W.padConfirm.show(el.area, a, res));
    W.padGestures.onDone(ran);
    W.padActions.onChange(() => { if (!root.hidden) W.padRail.paint(); });

    W.padKeys.onMods((m) => {
      for (const b of root.querySelectorAll("[data-mod]")) {
        const st = m[b.dataset.mod];
        b.classList.toggle("latched", st === "latched");
        b.classList.toggle("locked", st === "locked");
        b.setAttribute("aria-pressed", st === "off" ? "false" : "true");
      }
    });
    W.pad.onState(paintPill);
    W.pad.onMsg((m) => {
      if (m.k === "pong") paintPill();
      // The action list's refusal is shown where the actions would be (js/pad_actions.js).
      if (m.k === "err" && !W.padActions.owns(m)) { (m.fatal ? refuse : alert)(m.message); paintPill(); }
    });

    // First-touch hint fade. The rings, connector, chip and ambient highlight are js/pad_feel.js,
    // the gesture hints js/pad_hints.js, the first-use coach js/pad_coach.js.
    el.area.addEventListener("pointerdown", () => el.area.classList.add("touched"));

    // Which way round: the layout follows this device's settings and the keyboard.
    W.padPrefs.onChange((p) => { if (!root.hidden) { orient(p); W.padRail.paint(); } });
    W.padOrient.onChange(turned);
    W.padOrient.onNote((text) => {
      el.note.textContent = text;
      el.note.hidden = false;
      setTimeout(() => { el.note.hidden = true; }, 6000);
    });

    window.addEventListener("popstate", () => {
      if (root.hidden) return;
      if (W.padSettings.isOpen()) W.padSettings.hide(); // Back from the settings sheet
      else close(false);
    });
    if (window.visualViewport) {
      const fit = () => {
        if (root.hidden) return;
        // Drawn turned, the pad fills the screen as it is; the keyboard never shows then (typing
        // turns it upright), so there is nothing to fit around.
        if (W.padOrient.angle) { root.style.height = ""; root.style.transform = ""; return; }
        const vv = window.visualViewport;
        root.style.height = vv.height + "px";
        root.style.transform = `translateY(${vv.offsetTop}px)`;
      };
      window.visualViewport.addEventListener("resize", fit);
      window.visualViewport.addEventListener("scroll", fit);
    }
  }

  // Something that did not work (an action, a word typed): a note, gone on a tap.
  function alert(text) {
    el.err.replaceChildren();
    el.err.textContent = text;
    el.err.dataset.cause = "other";
    el.err.dataset.msg = "";
    el.err.hidden = false;
  }

  // The computer closed the pad: a card that says what to do (js/pad_why.js), never a bare pad.
  function refuse(text) {
    W.padWhyShow(el.err, text, retry);
  }

  // After the fix (an unlock, a grant, a setting): the same pad, linked again.
  function retry() {
    if (!root || root.hidden) return;
    el.err.hidden = true;
    W.pad.start();
    paintPill();
  }

  function paintPill() {
    const s = W.pad.state;
    const off = el.err.hidden ? "Connecting…" : "Not connected";
    el.pilltext.textContent = s === "direct" ? "Direct" : s === "relayed" ? "Relayed" : off;
    el.rtt.textContent = s !== "off" && W.pad.rtt != null ? `${W.pad.rtt} ms` : "";
    el.pill.classList.toggle("good", s === "direct");
    el.pill.classList.toggle("warn", s === "relayed");
    el.pill.classList.toggle("dim", s === "off");
    el.why.textContent = WHY[s] || WHY.off;
  }

  const orient = (p) => W.padOrient.update({ landscape: p.landscape, flip: p.flip, fullscreen: p.fullscreen });

  // The pad turned or changed layout: the rings in the old frame go, and it settles in.
  let opening = false;
  function turned() {
    if (root.hidden) return;
    W.padRail.paint();
    if (opening) return;
    W.padGestures.attach(el.area);
    W.padFeel.attach(el.area);
    W.padHints.attach(el.area);
    root.classList.remove("tpturn");
    void root.offsetWidth;
    root.classList.add("tpturn");
  }

  function keyboard(open) {
    // Typing turns the pad upright with the phone's keyboard (js/pad_orient.js), first, so the
    // field is focused in the layout it will be typed in.
    W.padOrient.update({ typing: open });
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
    const p = W.padPrefs.get();
    opening = true; // the open animation covers the first layout
    orient(p);
    W.padOrient.update({ typing: p.keyboard });
    W.padOrient.attach(root);
    W.pad.start();
    W.padActions.refresh();
    W.padPointer.attach(el.area);
    W.padTouch.attach(el.area);
    W.padGestures.attach(el.area);
    W.padFeel.attach(el.area);
    W.padHints.attach(el.area);
    W.padCoach.attach(el.area);
    W.padKeys.attach(el.text, el.row);
    paintPill();
    if (p.keyboard) keyboard(true);
    opening = false;
    if (W.wake) W.wake.acquire();
  };

  // `viaHistory`: closed by the button, so take back the entry `padOpen` pushed.
  function close(viaHistory) {
    if (!root || root.hidden) return;
    if (viaHistory && history.state && history.state.wadoPad) { history.back(); return; }
    W.padSettings.hide();
    W.padOrient.detach();
    keyboard(false);
    W.padPointer.detach();
    W.padTouch.detach();
    W.padGestures.detach();
    W.padFeel.detach();
    W.padHints.detach();
    W.padCoach.detach();
    W.padKeys.detach();
    W.pad.stop();
    root.classList.remove("tpturn");
    root.hidden = true;
    root.style.height = "";
    root.style.transform = "";
    document.documentElement.classList.remove("trackpad-open");
    if (W.wake && !W.sessionOn) W.wake.release();
  }
  W.padClose = () => close(true);
})();
