// wado bridge — the Android app's native trackpad (native.js; nothing without it).
//
// The app draws the pad and paces its frames itself (crates/mobile-core, the Rust port of
// js/pad_touch.js); this page only lends it the pad's link (js/pad_link.js) — relayed through
// the envelope, or direct on its own data channels — so there is one link, one set of keys.
//
//   app → page  padStart {key}        open the link to this computer's pad
//   app → page  pad      {msg, lossy} one PadMsg to send (lossy: the touch channel)
//   app → page  padStop               let go of everything, close the link
//   page → app  padState {state}      "off" | "relayed" | "direct" — the pacer's timing follows it
//   page → app  padMsg   {msg}        what the daemon said (err, ran, …)

(() => {
  const N = W.native;
  if (!N || !W.pad) return;
  let active = false;
  let resend = null;

  W.pad.onState((state) => { if (active) N.send("padState", { state }); });
  W.pad.onMsg((msg) => { if (active && msg.k !== "pong") N.send("padMsg", { msg }); });

  const currentKey = () => {
    const s = W.loadSettings();
    const p = (Array.isArray(s.profiles) ? s.profiles : [])[s.profile | 0];
    return p ? (p.relay_url || "") + "|" + (p.remote_id || "") : "";
  };

  N.on("padStart", async (m) => {
    if (m.key && m.key !== currentKey()) emit({ type: "nativeGo", key: m.key, go: "" });
    active = true;
    // The web pad's surface is not up: its re-send on a path change would send an empty frame
    // and lift the app's fingers. The app re-sends its own on `padState`.
    if (W.padTouch && !resend) { resend = W.padTouch.resend; W.padTouch.resend = () => {}; }
    const t0 = Date.now();
    while (!W.relayUp && active && Date.now() - t0 < 25000) await new Promise((r) => setTimeout(r, 150));
    if (!active) return;
    if (!W.relayUp) { N.send("padMsg", { msg: { k: "err", message: "The computer didn't answer.", fatal: true } }); return; }
    W.pad.start();
    N.send("padState", { state: W.pad.state });
  });
  N.on("pad", (m) => { if (active && m.msg) W.pad.send(m.msg, !!m.lossy); });
  N.on("padStop", () => {
    if (!active) return;
    active = false;
    W.pad.stop();
    if (resend) { W.padTouch.resend = resend; resend = null; }
  });
})();
