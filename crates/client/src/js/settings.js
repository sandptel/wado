// wado bridge — client-side settings setters, called from the Rust UI (Compositor settings /
// Debug sections) via tiny one-shot evals. These adjust live input behaviour instantly;
// compositor-side settings (keyboard repeat, placement, focus policy) ride SessionConfig at
// Start instead. (W.setShowTouches lives in overlay.js, which owns the overlay.)

W.setMoveMode = (on) => { W.moveMode = !!on; };
// Live, mid-session: ticking the box while a session runs must release a strain latch
// already sent, which it does — `reportStrain` sends the change on the next health tick.
W.setFpsLock = (on) => { W.fpsLock = !!on; };
W.setScroll = (speed, natural) => {
  W.scrollSpeed = +speed || 1;
  W.naturalScroll = !!natural;
};
