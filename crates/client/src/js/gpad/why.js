// wado bridge — why the gamepad was refused: the trackpad's own refusal card (`js/pad_why.js`),
// said about a controller. The gamepad rides the same link, gate and passkey (spec D3), so the
// causes and fixes are the same; only the words change. `pad_why.js` stays the one reader of the
// daemon's sentences — this only rewrites the card it drew.
//
//   W.gpadWhy(w)                     → the same {cause, title, body, …}, about a controller
//   W.gpadWhyShow(el, message, retry)  fill `el` like `W.padWhyShow`
//   W.gpadTooOld(el)                 the card for a daemon that cannot carry the gamepad (§5.9)

(() => {
  const say = (t) => t && t
    .replace(/the trackpad and keyboard/g, "the gamepad")
    .replace(/The trackpad and typing are/g, "The gamepad, the trackpad and typing are")
    .replace(/trackpad/g, "gamepad");

  W.gpadWhy = (w) => {
    const out = { ...w, title: say(w.title), body: say(w.body) };
    if (w.cause === "uinput") {
      out.title = "The computer can't make a controller yet";
      out.body = "wado makes a real controller through /dev/uinput, and this computer doesn't let it open that. On the computer, run these once:";
    }
    if (w.cause === "off") {
      out.body = "Its config has trackpad { enabled #false }, which turns off the touchpad, keyboard and gamepad. To turn them on, run on the computer:";
    }
    if (w.cause === "unsupported") out.body = "wado can only make a controller on Linux for now.";
    return out;
  };

  W.gpadWhyShow = (el, message, retry) => {
    const w = W.padWhyShow(el, message, retry);
    const g = W.gpadWhy(w);
    const t = el.querySelector(".tpwhyt"), b = el.querySelector(".tpwhyb");
    if (t) t.textContent = g.title;
    if (b) b.textContent = g.body;
    return g;
  };

  W.gpadTooOld = (el) => {
    el.replaceChildren();
    el.dataset.cause = "old";
    el.dataset.msg = "old";
    const b = document.createElement("b");
    b.className = "tpwhyt";
    b.textContent = "This computer's wado is too old for the gamepad away from a session";
    const p = document.createElement("p");
    p.className = "tpwhyb";
    p.textContent = "Update wado on the computer, or use the gamepad inside a session.";
    el.append(b, p);
    el.hidden = false;
  };
})();
