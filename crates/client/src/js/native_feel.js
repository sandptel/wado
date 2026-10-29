// wado bridge — what the Android app adds to the hand (native.js; nothing without it).
//
//   page → app  haptic  {kind: "tick"|"tap"|"claim"|"confirm"|"reject"|"heavy"}
//                       the phone's own predefined effects, not a length of buzz
//   page → app  active  {on}           a session or the trackpad is up: keep the screen on, go
//                                      immersive (no status or navigation bar)
//   page → app  orient  {lock: "landscape"|"reverse"|"none"}   a real orientation lock, no
//                                      fullscreen needed (js/pad_orient.js asks)
//
// The existing `navigator.vibrate(ms)` calls (home's long-press, a widget's drag) become the
// nearest predefined effect, so they feel like the rest of the phone instead of a motor.

(() => {
  const N = W.native;
  if (!N) return;

  const haptic = (kind) => N.send("haptic", { kind });
  W.native.haptic = haptic;
  try {
    navigator.vibrate = (p) => {
      const ms = Array.isArray(p) ? p.reduce((a, b) => a + (b | 0), 0) : p | 0;
      if (ms > 0) haptic(ms <= 6 ? "tick" : ms <= 15 ? "tap" : "heavy");
      return true;
    };
  } catch (_) {}

  // The trackpad: an action that ran (a gesture, the rail) confirms or refuses in the hand as
  // well as on screen.
  if (W.padConfirm && W.padConfirm.show) {
    const show = W.padConfirm.show;
    W.padConfirm.show = (area, action, res, at) => {
      haptic(res && res.ok === false ? "reject" : "confirm");
      return show(area, action, res, at);
    };
  }
  // …and a tap — one finger down and up quickly without travelling — ticks like a touchpad's
  // click. The tap itself is the computer's (libinput); this only mirrors what it will see.
  if (W.padTouch && W.padTouch.input) {
    const input = W.padTouch.input;
    const downs = new Map();
    W.padTouch.input = (type, p) => {
      if (type === "down") downs.set(p.id, { x: p.x, y: p.y, t: performance.now(), n: downs.size + 1 });
      else if (type === "up") {
        const d = downs.get(p.id);
        downs.delete(p.id);
        if (d && d.n === 1 && downs.size === 0 && performance.now() - d.t < 180 &&
            Math.hypot(p.x - d.x, p.y - d.y) < 10) haptic("tap");
      }
      return input(type, p);
    };
  }
  // Screen on + immersive while a session or the trackpad is up: the same moments the page
  // takes a wake lock (js/wakelock.js), which a WebView does not grant.
  let active = false;
  const setActive = (on) => { if (on !== active) { active = on; N.send("active", { on }); } };
  const acquire = W.wake.acquire, release = W.wake.release;
  W.wake.acquire = async () => { if (!W.wake.off) setActive(true); return acquire(); };
  W.wake.release = () => { setActive(false); return release(); };
  W.setWake = ((setWake) => (on) => { setWake(on); if (!on) setActive(false); })(W.setWake);
})();
