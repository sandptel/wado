// wado bridge — the gamepad's wire with no session (§10.2): the pad link (`js/pad_link.js`) carries
// it to the daemon's uinput controller (`crates/server/src/pad/game.rs`).
//
//   game_on / game_off        keys channel, reliable: make the controller / let go of it
//   btn {code, down}          keys channel, reliable, ordered: buttons, the hat as BTN_DPAD_*,
//                             digital triggers as BTN_TL2 / BTN_TR2
//   axes {seq, a:[lx,ly,rx,ry,lt,rt]}   touch channel, unreliable, newest-wins: the WHOLE axis
//                             state, re-sent every 100 ms while anything is off-centre. The daemon
//                             centres the axes after 250 ms of silence, so a lost "back to centre"
//                             can never leave a stick held.
//
//   W.gpadSendPad() → sender for `W.gpadController`, plus .start() / .stop() / .resend()

(() => {
  const CODE = {
    a: 0x130, b: 0x131, x: 0x133, y: 0x134, lb: 0x136, rb: 0x137, lt: 0x138, rt: 0x139,
    back: 0x13a, start: 0x13b, guide: 0x13c, l3: 0x13d, r3: 0x13e,
    du: 0x220, dd: 0x221, dl: 0x222, dr: 0x223,
  };
  const KEEPALIVE_MS = 100;
  W.gpadPadCodes = CODE;

  W.gpadSendPad = () => {
    let seq = 0;
    let timer = 0;
    let last = { lx: 0, ly: 0, rx: 0, ry: 0, lt: 0, rt: 0 };
    const held = new Set();
    const send = (m, lossy) => W.pad && W.pad.send(m, lossy);
    const frame = (a) => ({
      k: "axes",
      seq: (seq = (seq + 1) >>> 0),
      a: [a.lx, a.ly, a.rx, a.ry].map((v) => Math.round(v * 32767)).concat([a.lt, a.rt].map((v) => Math.round(v * 255))),
    });
    const off = (a) => Object.values(a).some((v) => v !== 0);
    function keepAlive() {
      clearInterval(timer);
      timer = 0;
      if (off(last)) timer = setInterval(() => send(frame(last), true), KEEPALIVE_MS);
    }

    return {
      start() { send({ k: "game_on" }, false); },
      stop() {
        clearInterval(timer);
        timer = 0;
        send({ k: "game_off" }, false);
      },
      // The path changed (relay → direct): the daemon's picture is re-sent whole.
      resend() {
        send({ k: "game_on" }, false);
        for (const b of held) send({ k: "btn", code: CODE[b], down: true }, false);
        send(frame(last), true);
      },
      button(bind, down) {
        if (CODE[bind] === undefined) return;
        if (down) held.add(bind); else held.delete(bind);
        send({ k: "btn", code: CODE[bind], down }, false);
      },
      axes(a) {
        last = { ...a };
        send(frame(last), true);
        keepAlive();
      },
      release() {
        held.clear();
      },
    };
  };
})();
