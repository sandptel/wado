// wado bridge — tap targets: what the app's accessibility tree says is under a finger (M-P S5).
//
// One job: ask on touch-down, remember the answer, and hand it to the tap when the finger
// lifts. Asking on *down* is the point: the server answers in ~10–20 ms plus the network, so by
// the time a tap's finger comes up the answer is usually already here and costs no latency.
// A tap never waits more than TARGET_WAIT_MS past lift for it — an absent answer is a plain tap.
//
// Relay only (the direct transport has no push path yet): in direct mode every answer is
// `null`, "no tree", and the tap falls back to the pixel heuristic in lens.js.

// Fingertip radius in CSS pixels — ~7 mm across on a phone.
const FINGER_CSS = 22;
// How long a lifted tap will wait for an answer still in flight.
const TARGET_WAIT_MS = 70;

W.targets = {
  seq: 0,
  pending: new Map(), // seq → { resolve, done, value }

  // The video's content rect (object-fit: contain), in CSS pixels — as normPoint computes it.
  content(video) {
    const r = video.getBoundingClientRect();
    const vw = video.videoWidth, vh = video.videoHeight;
    if (!vw || !vh) return null;
    const s = Math.min(r.width / vw, r.height / vh);
    const w = vw * s, h = vh * s;
    return { left: r.left + (r.width - w) / 2, top: r.top + (r.height - h) / 2, w, h };
  },

  // Ask about the point under a finger that has just come down. Returns a ticket for `await`.
  ask(clientX, clientY, video) {
    const n = W.normPoint(clientX, clientY, video);
    const c = W.targets.content(video);
    const seq = ++W.targets.seq & 0xffffffff;
    const entry = { done: false, value: null, resolve: null };
    W.targets.pending.set(seq, entry);
    const ws = W.relayWs;
    if (!n || !c || !W.relayMode || !ws || ws.readyState !== WebSocket.OPEN) {
      entry.done = true; // nothing to ask with: "no tree"
    } else {
      ws.send(JSON.stringify({ type: "targets_request", seq, x: n.x, y: n.y, r: FINGER_CSS / c.w }));
    }
    // Old tickets are never awaited once their tap is over; keep the map from growing.
    if (W.targets.pending.size > 16) W.targets.pending.delete(W.targets.pending.keys().next().value);
    return seq;
  },

  // The answer for a ticket: an array of targets, [] for "nothing actionable", or null for
  // "no tree / no answer in time".
  wait(seq) {
    const entry = W.targets.pending.get(seq);
    if (!entry) return Promise.resolve(null);
    if (entry.done) return Promise.resolve(entry.value);
    return new Promise((resolve) => {
      entry.resolve = resolve;
      setTimeout(() => { if (entry.resolve) { entry.resolve = null; resolve(null); } }, TARGET_WAIT_MS);
    });
  },

  answer(seq, targets) {
    const entry = W.targets.pending.get(seq);
    if (!entry) return;
    entry.done = true;
    entry.value = targets === undefined ? null : targets;
    if (entry.resolve) { const r = entry.resolve; entry.resolve = null; r(entry.value); }
  },
};

W.relayOn("targets", (msg) => W.targets.answer(msg.seq, msg.targets));

// What a tap on normalized point `n` should do, given the targets near it (pure — see
// scripts/lens-check.mjs). `size` converts a normalized w/h to CSS pixels.
//   → { kind: "click", at }  plain click (nothing actionable, or one big target under it)
//   → { kind: "snap", at }   exactly one target near: click its centre (the magnetic tap)
//   → { kind: "lens" }       several compete and one is small: let the lens decide
W.decideTap = (n, targets, size, smallCss = 44) => {
  if (!targets || targets.length === 0) return { kind: "click", at: n };
  const centre = (t) => ({ x: t.x + t.w / 2, y: t.y + t.h / 2 });
  if (targets.length === 1) return { kind: "snap", at: centre(targets[0]) };
  const inside = targets.filter((t) => n.x >= t.x && n.x < t.x + t.w && n.y >= t.y && n.y < t.y + t.h);
  const small = (t) => Math.min(size(t).w, size(t).h) < smallCss;
  if (inside.length === 1 && !small(inside[0])) return { kind: "click", at: n };
  return targets.some(small) ? { kind: "lens" } : { kind: "click", at: n };
};
