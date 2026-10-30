// wado bridge — the link speed test (Decision Log 2026-10-30): before this device starts a
// session or joins one, pull a burst of filler from the daemon over the link it is on, time it,
// and ask the daemon what that means. The daemon judges (`server::linktest::verdict`) — this file
// only measures and carries the answer.
//
//   W.linkTest(intent) → Promise<verdict | null>   intent "join" | "start"; null: the daemon is
//                                                   older than the test (no answer), go ahead.
//
// Emits {type:"linkTest", phase:"run"|"done", verdict, intent} for the Sessions section's meter.

const LINKTEST_BYTES = 384 * 1024;
const LINKTEST_WAIT_MS = 8000;

let linkTestRun = null; // {resolve, t0, first, bytes, chunks, timer, intent}

W.linkTest = (intent) => {
  if (linkTestRun) return linkTestRun.promise;
  let resolve;
  const promise = new Promise((r) => { resolve = r; });
  const run = { resolve, promise, t0: performance.now(), first: 0, bytes: 0, intent: intent || "join", timer: 0 };
  const finish = (verdict) => {
    clearTimeout(run.timer);
    if (linkTestRun === run) linkTestRun = null;
    emit({ type: "linkTest", phase: "done", intent: run.intent, verdict: verdict || null });
    resolve(verdict || null);
  };
  run.finish = finish;
  run.timer = setTimeout(() => {
    if (W.rlog) W.rlog("speed test: no answer — the computer's wado is older than the test");
    finish(null);
  }, LINKTEST_WAIT_MS);
  linkTestRun = run;
  emit({ type: "linkTest", phase: "run", intent: run.intent });
  if (!W.relaySendMsg({ type: "link_test", bytes: LINKTEST_BYTES })) finish(null);
  return promise;
};

W.relayOn("link_test_chunk", (m) => {
  const run = linkTestRun;
  if (!run) return;
  const now = performance.now();
  // The first chunk's arrival is the round trip (plus one chunk); the rest is throughput.
  if (!run.first) { run.first = now; return; }
  run.bytes += (m.pad || "").length;
  run.last = now;
});

W.relayOn("link_test_done", () => {
  const run = linkTestRun;
  if (!run) return;
  const span = Math.max(1, (run.last || performance.now()) - (run.first || run.t0));
  const down = Math.round((run.bytes * 8) / span); // bits per ms = kbps
  const rtt = Math.round((run.first || performance.now()) - run.t0);
  if (W.rlog) W.rlog("speed test: " + down + " kbps down, " + rtt + " ms to first byte");
  W.relaySendMsg({ type: "link_report", down_kbps: down, rtt_ms: rtt, intent: run.intent });
});

// The daemon did not understand `link_test` (older than it): no verdict, at once.
W.linkTestAbort = () => { if (linkTestRun) linkTestRun.finish(null); };

W.relayOn("link_verdict", (m) => {
  if (linkTestRun) linkTestRun.finish(m.verdict || null);
});
