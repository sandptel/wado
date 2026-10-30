// wado bridge — where the relay's limited lane is felt, say so (Decision Log 2026-10-27: hints
// "at the exact spots the cap hurts … saying it is the network, not wado"). One job: notice the
// spots, and tell Rust; the words and the buttons are `crates/client/src/lane.rs` and
// `ui/lane_hint.rs`. The third spot — no picture — needs no detector: the link badge's
// `relayed` state during a session is it.
//
//   W.laneHint(spot, detail)    show the hint for `spot` ("files" | "shell"); detail: the
//                               computer's own words, or "" / "stalled"
//   W.laneHintClear(spot)
//
// Detectors:
//   shell  output arriving at the lane's pace — over 40 KB/s for 2 s or more. Shells always ride
//          the relay link, so this fires on a direct link too; the words then say so instead of
//          blaming the network.
//   files  on the lane (files_link.js says so), a lane refusal (`lane: true`), or a transfer that
//          has not moved for STALL_MS.
//
// To Rust: {type:"laneHint", spot, on, detail}.

(() => {
  const shown = {};
  W.laneHint = (spot, detail) => {
    const d = detail || "";
    if (shown[spot] === d) return;
    shown[spot] = d;
    emit({ type: "laneHint", spot, on: true, detail: d });
  };
  W.laneHintClear = (spot) => {
    if (!(spot in shown)) return;
    delete shown[spot];
    emit({ type: "laneHint", spot, on: false, detail: "" });
  };

  // ── shell: a big dump ─────────────────────────────────────────────────────────────────
  const WINDOW_MS = 3000, RATE = 40 * 1024, QUIET_MS = 4000;
  let seen = []; // [t, bytes]
  let quiet = 0;
  const prev = W._relayHandlers.pty_output;
  W.relayOn("pty_output", (m) => {
    if (prev) prev(m);
    if (m.replay) return;
    const now = performance.now();
    seen.push([now, (m.data || "").length]);
    while (seen.length && now - seen[0][0] > WINDOW_MS) seen.shift();
    const span = now - seen[0][0];
    const bytes = seen.reduce((a, s) => a + s[1], 0);
    if (span >= 2000 && bytes / (span / 1000) >= RATE) W.laneHint("shell", "");
    clearTimeout(quiet);
    quiet = setTimeout(() => { seen = []; W.laneHintClear("shell"); }, QUIET_MS);
  });

  // ── files: a transfer that does not move ──────────────────────────────────────────────
  const STALL_MS = 8000;
  const moved = new Map(); // xfer → [done, at]
  setInterval(() => {
    const F = W.files;
    if (!F || !F.xfers) return;
    const now = performance.now();
    let stalled = false, running = false;
    for (const x of F.xfers) {
      if (x.state !== "running") { moved.delete(x); continue; }
      running = true;
      const m = moved.get(x);
      if (!m || m[0] !== x.done) moved.set(x, [x.done, now]);
      else if (now - m[1] > STALL_MS) stalled = true;
    }
    if (stalled) W.laneHint("files", "stalled");
    else if (shown.files === "stalled") {
      // Moving again: back to the lane's own hint if that is where files are, else nothing.
      if (F.lane) W.laneHint("files", ""); else W.laneHintClear("files");
    }
    if (!F.lane && !running && shown.files === "") W.laneHintClear("files");
  }, 1000);
})();
