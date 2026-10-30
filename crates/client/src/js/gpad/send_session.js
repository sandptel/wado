// wado bridge — the gamepad's wire inside a session, unchanged from before the layout model
// (§10.1): the session's own input channel, through `W.coalesce`.
//
//   "pad"   a real virtual controller made by the compositor (`gamepad_button`/`gamepad_axis`,
//           raw evdev codes — wado_protocol::InputEvent::GamepadButton).
//   "keys"  keyboard keys and mouse buttons on the input path that already exists: the left
//           stick walks (WASD), the right stick looks (relative mouse), A jumps… Nothing is needed
//           on the host, and it is the mode that drives a game that aims with the mouse.
//
// Everything terminal goes through `W.coalesce.now`, which flushes anything queued first: a press
// that overtook a pending stick sample would arrive before the movement that led to it.
//
//   W.gpadSendSession(mode) → sender for `W.gpadController`

(() => {
  const BTN = { a: 0x130, b: 0x131, x: 0x133, y: 0x134, lb: 0x136, rb: 0x137, back: 0x13a, start: 0x13b, guide: 0x13c, l3: 0x13d, r3: 0x13e };
  const ABS = { lx: 0x00, ly: 0x01, lt: 0x02, rx: 0x03, ry: 0x04, rt: 0x05, hx: 0x10, hy: 0x11 };
  const STICK_MAX = 32767;
  // What each control does in "keys" mode — the near-universal first-person bindings. Overridable
  // per control in the editor's Behave tab later; this is the default that works the first time.
  const KEYMAP = {
    a: { key: 57 }, b: { key: 29 }, x: { key: 18 }, y: { key: 19 },
    lb: { mouse: "right" }, rb: { mouse: "left" }, lt: { key: 16 }, rt: { key: 33 },
    l3: { key: 42 }, r3: { key: 47 }, back: { key: 15 }, start: { key: 1 }, guide: { key: 125 },
    du: { key: 103 }, dd: { key: 108 }, dl: { key: 105 }, dr: { key: 106 },
  };
  const WASD = { up: 17, left: 30, down: 31, right: 32 };
  const WALK_DEADZONE = 0.35;
  const LOOK_SPEED = 14;
  W.gpadKeymap = KEYMAP;

  W.gpadSendSession = (mode) => {
    const hat = { du: 0, dd: 0, dl: 0, dr: 0 };
    const walk = {};
    const look = { x: 0, y: 0, raf: 0 };
    const trig = { lt: false, rt: false };
    const now = (m) => W.coalesce && W.coalesce.now(m);

    function keyOrMouse(m, down) {
      if (!m) return;
      if (m.mouse) now({ t: "button", x: 0.5, y: 0.5, button: m.mouse, pressed: down });
      else now({ t: "key", code: m.key, pressed: down });
    }
    function lookLoop() {
      if (look.raf) return;
      const tick = () => {
        if (!look.x && !look.y) { look.raf = 0; return; }
        W.coalesce.add("pointer_relative", { t: "pointer_relative", dx: look.x * LOOK_SPEED, dy: look.y * LOOK_SPEED });
        look.raf = requestAnimationFrame(tick);
      };
      look.raf = requestAnimationFrame(tick);
    }

    return {
      mode,
      button(bind, down) {
        if (mode === "keys") return keyOrMouse(KEYMAP[bind], down);
        if (bind in hat) {
          hat[bind] = down ? 1 : 0;
          const vert = bind === "du" || bind === "dd";
          const v = vert ? hat.dd - hat.du : hat.dr - hat.dl;
          return now({ t: "gamepad_axis", code: vert ? ABS.hy : ABS.hx, value: v });
        }
        // Digital triggers are an axis on a real controller: a game reading LT as an axis would
        // never see a press sent as a button.
        if (bind === "lt" || bind === "rt") return now({ t: "gamepad_axis", code: ABS[bind], value: down ? 255 : 0 });
        if (BTN[bind] !== undefined) now({ t: "gamepad_button", code: BTN[bind], pressed: down });
      },
      axes(a, changed) {
        if (mode === "keys") {
          if (changed.includes("lx") || changed.includes("ly")) {
            const want = { up: a.ly < -WALK_DEADZONE, down: a.ly > WALK_DEADZONE, left: a.lx < -WALK_DEADZONE, right: a.lx > WALK_DEADZONE };
            for (const d in WASD) {
              if (!!walk[d] === want[d]) continue;
              walk[d] = want[d];
              now({ t: "key", code: WASD[d], pressed: want[d] });
            }
          }
          if (changed.includes("rx") || changed.includes("ry")) { look.x = a.rx; look.y = a.ry; lookLoop(); }
          for (const t of ["lt", "rt"]) {
            if (!changed.includes(t) || trig[t] === a[t] > 0.5) continue;
            trig[t] = a[t] > 0.5;
            keyOrMouse(KEYMAP[t], trig[t]);
          }
          return;
        }
        for (const k of changed) {
          const v = k === "lt" || k === "rt" ? Math.round(a[k] * 255) : Math.round(a[k] * STICK_MAX);
          // An axis is state: queued, newest-wins, and flushed before the next terminal event.
          W.coalesce.queue(`pad-${k}`, { t: "gamepad_axis", code: ABS[k], value: v });
        }
      },
      release() {
        look.x = look.y = 0;
      },
    };
  };
})();
