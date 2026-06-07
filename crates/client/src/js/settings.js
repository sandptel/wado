// wado bridge — client-side settings setters, called from the Rust UI (Compositor settings /
// Debug sections) via tiny one-shot evals. These adjust live input behaviour instantly;
// compositor-side settings (keyboard repeat, placement, focus policy) ride SessionConfig at
// Start instead. (W.setShowTouches lives in overlay.js, which owns the overlay.)

W.setMoveMode = (on) => { W.moveMode = !!on; };
W.setScroll = (speed, natural) => {
  W.scrollSpeed = +speed || 1;
  W.naturalScroll = !!natural;
};
