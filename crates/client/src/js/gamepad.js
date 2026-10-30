// wado bridge — the on-screen gamepad inside a session: the session's layout (this device's
// `gamepad { layout }`, `gpad/prefs.js`) drawn by the one renderer (`gpad/render.js`) over the
// picture in the cockpit, or packed into the Pocket Console's deck when its face is Gamepad
// (`W.gpadPlace.deck`, the `pad` slot `ui/surround/layout.rs` computes). Presses go out on the
// session's own input channel in either of two modes (`gpad/send_session.js`):
//
//   "pad"   a real virtual controller made by the compositor (an Xbox 360 pad on the host)
//   "keys"  keyboard keys and mouse buttons — nothing needed on the host; aims with the mouse
//
// The mode is this device's `gamepad { drives }`. Editing happens in the full-screen editor
// (`W.gpadOpen({where: "session", state: "edit"})`), never on top of the game.
//
// The container is pointer-events:none and only the controls take input, so a tap in the gaps
// between them still reaches the video underneath.
//
//   W.setGamepad({on})      the one entry point Rust calls (other fields are ignored)
//   W.setPadEdit(on)        opens the editor on the session's layout
//   W.resetPadLayout()      the session's layout back to its built-in
//   W.gamepad.releaseAll()  everything held goes up

(() => {
  const P = W.gpadPlace;
  // Over the picture the pad is see-through on top of its own See-through: anything solid enough
  // to read comfortably is solid enough to hide what you are aiming at.
  const OVER_PICTURE = 0.62;
  W.padCfg = { on: false };
  const G = (W.gamepad = { root: null, ctl: null, L: null, mode: "" });

  const console_ = () => {
    const d = document.documentElement.dataset;
    return d.arr === "console" && d.face === "gamepad";
  };

  function build() {
    if (G.root) return G.root;
    // The portal Dioxus renders for exactly this — see ui/stage.rs. Absent until the app has
    // mounted, so `apply` is safe to call before then and simply does nothing.
    const mount = document.getElementById("wado-pad-mount");
    if (!mount) return null;
    const root = document.createElement("div");
    root.id = "wado-pad";
    root.hidden = true;
    root.setAttribute("role", "application");
    root.setAttribute("aria-label", "Gamepad");
    mount.appendChild(root);
    G.root = root;
    G.ctl = W.gpadController(null);
    W.gpadInput.attach(root, {
      get ctl() { return G.ctl; },
      layout: () => G.L,
      haptics: () => W.gpadPrefs.get().haptics,
      editing: () => false,
    });
    addEventListener("resize", () => draw());
    // The console ⇄ cockpit morph and a face change move the pad's box: lay out again, flying
    // each control from where it was (spec §4: clusters fly between the deck and the corners).
    new MutationObserver(() => draw(true)).observe(document.documentElement, { attributes: true, attributeFilter: ["data-arr", "data-face", "data-cdeck"] });
    W.gpadPrefs.onChange(() => { G.L = null; draw(true); });
    return root;
  }

  // In a landscape console the pad's slot surrounds the picture: its rect, inside the slot.
  function picInPad() {
    const F = W.stageFit, pad = F && F.slot("pad"), pic = F && F.slot("pic");
    if (!pad || !pic || document.documentElement.dataset.cdeck !== "col") return null;
    return { x: pic.x - pad.x, y: pic.y - pad.y, w: pic.w, h: pic.h };
  }

  function draw(morph) {
    const root = G.root;
    if (!root || root.hidden) return;
    const prefs = W.gpadPrefs.get();
    if (G.mode !== prefs.drives) {
      G.ctl.releaseAll();
      G.mode = prefs.drives;
      G.ctl.setSender(W.gpadSendSession(G.mode));
    }
    const L = (G.L = G.L || W.gpadPrefs.layoutFor("session"));
    const w = root.clientWidth, h = root.clientHeight;
    if (!w || !h) return;
    const deck = console_();
    const o = P.orient(innerWidth, innerHeight);
    const first = morph && root._gp ? W.gpadMotion.rects(root._gp) : null;
    W.gpadRender(root, L, {
      o, k: 1, live: true, w, h,
      place: deck ? P.deck(L, w, h, picInPad()) : P.all(L, o, 1, w, h),
      opacity: deck ? 1 : OVER_PICTURE,
    });
    if (first) W.gpadMotion.morph(root, first, []);
  }

  G.apply = () => {
    const root = build();
    if (!root) return;
    const on = !!W.padCfg.on;
    if (root.hidden !== !on) {
      root.hidden = !on;
      if (!on) G.releaseAll();
      else { draw(); if (root._gp) W.gpadMotion.assemble(root._gp, P.orient(innerWidth, innerHeight)); }
    } else draw();
  };

  // Everything down goes up: when the pad is switched off and when the session ends, a control
  // still held is a key nothing can ever release.
  G.releaseAll = () => {
    if (!G.root) return;
    W.gpadInput.releaseAll(G.root);
    if (G.ctl) G.ctl.releaseAll();
    for (const el of G.root.querySelectorAll(".gpc.on")) el.classList.remove("on");
  };

  W.setGamepad = (cfg) => {
    if (cfg && "on" in cfg) W.padCfg.on = !!cfg.on;
    G.apply();
  };

  W.setPadEdit = (on) => {
    if (!on) return;
    G.releaseAll();
    W.gpadOpen({ where: "session", state: "edit" });
  };

  W.resetPadLayout = () => {
    const id = W.gpadPrefs.idFor("session");
    const base = id.startsWith("mine-") ? W.gpadPrefs.layoutOf(id).base : id;
    W.gpadPrefs.select("session", base);
    G.L = null;
    draw(true);
  };
})();
