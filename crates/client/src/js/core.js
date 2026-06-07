// wado bridge — core state + Rust messaging.
//
// The bridge is one long-lived Dioxus `eval`, assembled from these single-job files
// (concatenated in main.rs). This file owns the shared `window.__wado` (`W`) state object
// and the `emit`/`status`/`stagebar` helpers every other file uses to talk back to Rust via
// `dioxus.send`. Messages are `{type, ...}`:
//   {type:"status",   text}        – short status line
//   {type:"stagebar", text}        – the bar above the video
//   {type:"log",      line}        – one SSE log line (LEVEL|HH:MM:SS|text)
//   {type:"startFailed"}           – /session/start rejected or threw
//   {type:"giveup"}                – WebRTC failed past the retry budget; tear the session down
//   {type:"stats", fps, ping}      – once/sec live decode FPS + transport RTT (ms); may be null

window.__wado = window.__wado || {};
const W = window.__wado;

// Transport / lifecycle state.
W.pc = null;
W.logES = null;
W.server = "";
W.sessionOn = false;
W.reconnectAttempts = 0;
W.MAX_RECONNECTS = 3;
W.statsTimer = null;

// Input state.
W.inputDC = null;             // reliable+ordered data channel carrying InputEvents
W.pressedKeys = new Set();    // evdev codes currently down (for release-on-blur)
W.activePointers = new Set(); // pointerIds currently down
W.inputCaptureReady = false;  // DOM listeners attached once
W.gesture = null;             // touch gesture FSM for the primary contact
W.mouseDragging = false;      // a move-mode mouse drag is in progress

// Settings state (mirrored from the Rust UI via the setters in settings.js / overlay.js).
W.moveMode = false;
W.showTouches = false;
W.scrollSpeed = 1.0;
W.naturalScroll = false;

const emit = (msg) => { try { dioxus.send(msg); } catch (_) {} };
const status = (text) => emit({ type: "status", text });
const stagebar = (text) => emit({ type: "stagebar", text });
