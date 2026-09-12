#!/usr/bin/env node
// graceful-probe — a headless viewer that proves a session outlives its viewer.
//
// It speaks the relay wire protocol and nothing else: no WebRTC, no browser. That is the
// point. The question "does a dropped socket kill the desktop?" is answered entirely on the
// signalling plane, and answering it without a phone is what makes it a check rather than a
// field report.
//
// What it does, in order:
//   1. join  → session_start        expect session_started
//   2. session_launch               expect the app's pid to appear under the daemon
//   3. hard-close the WebSocket     the relay sends peer_disconnected
//   4. wait, then re-join           expect session_alive  ← the whole test
//   5. session_rejoin               expect session_started, same session
//   6. count the app pids again     expect the identical set
//   7. session_reconfigure          new shape, same pids
//   8. session_stop                 leave nothing behind
//
// Usage: node scripts/graceful-probe.mjs [ws://127.0.0.1:4000] [872990894]
//
// Exit 0 on pass, 1 on fail, and it says which step failed and what it saw instead.

import { execFileSync } from "node:child_process";
import { readFileSync } from "node:fs";

const RELAY = process.argv[2] || "ws://127.0.0.1:4000";
const ID = (process.argv[3] || "872990894").replace(/[\s-]/g, "");
const GAP_MS = Number(process.env.GAP_MS || 5000);
// How long to wait for the launched application to actually be up. A terminal is ready in well
// under a second; a browser is not.
const SETTLE_MS = Number(process.env.WADO_PROBE_SETTLE || 1500);

const CONFIG = {
  width: 1280, height: 720, fps: 60, scale: 1.0, quality: "balanced",
};
// Something that stays up and is trivially greppable. `sleep` is a child of the session's
// process group, which is exactly the thing `stop_session` kills — so its survival is the
// measurement, not a proxy for it.
// A unique sleep duration is the whole trick: it makes an ordinary `sleep` greppable without
// needing `exec -a`, which is a bashism and `proc::spawn` runs commands through `sh`.
const MARKER = String(9000 + Math.floor(Math.random() * 900));
// Wrapped in a real Wayland client when one is to hand, because the CPU A/B below is
// meaningless without a window: rendering an empty scene is nearly free whether it is paused or
// not, so a bare `sleep` measures the saving as far smaller than it is. The marker still works —
// `kitty -e sleep N` matches the same suffix, and the sleep is in the same process group.
// `WADO_PROBE_APP` picks the Wayland client. It must contain %M, which is replaced by the
// marker — that is what makes the process findable without a loose pattern match. An empty
// value launches a bare `sleep`, which tests process-group survival with no Wayland client at
// all, and is the control case for anything that looks protocol-related.
const APP = process.env.WADO_PROBE_APP ?? "kitty -e sleep %M";
const LAUNCH = (APP || "sleep %M").replaceAll("%M", MARKER);

let failed = null;
const step = (n, msg) => console.log(`  ${n}. ${msg}`);
const fail = (msg) => { if (!failed) failed = msg; console.log(`  ✖ ${msg}`); };

// pids of the marker process. Exact-args match, never a loose pattern — a loose one matches
// this script's own command line, which has produced two false positives in this project.
// Did the session's applications survive?
//
// **Not** "is the pid set identical". A browser is a process tree that spawns and reaps utility
// processes constantly, so an exact-set comparison fails on ordinary Chrome churn and says the
// session died when it did not — which it did, on the first run against Chrome. The question is
// whether the process that was *launched* is still running; everything under it is its business.
function survived(before, now) {
  return before.length > 0 && now.includes(before[0]);
}

function markerPids() {
  try {
    const out = execFileSync("ps", ["-eo", "pid=,args="], { encoding: "utf8" });
    return out.split("\n")
      .filter((l) => l.includes(MARKER) && !l.includes("graceful-probe"))
      .map((l) => l.trim().split(/\s+/)[0])
      .sort();
  } catch { return []; }
}

// Daemon CPU over a window, as a percentage of one core. utime+stime from /proc/<pid>/stat,
// which is the only reading here that is not a self-report.
//
// This is the A/B that says whether pausing the render tick was worth doing, and the probe can
// take both halves without a browser: between `session_start` and the disconnect the session is
// marked attached with no viewer actually receiving — which is exactly the old behaviour, a
// session rendering and encoding for nobody. After the disconnect it is paused. Same daemon,
// same session, seconds apart.
function daemonPid() {
  try { return execFileSync("pgrep", ["-x", "wado"], { encoding: "utf8" }).trim().split("\n")[0]; }
  catch { return null; }
}
function cpuTicks(pid) {
  try {
    const f = readFileSync(`/proc/${pid}/stat`, "utf8").split(" ");
    return Number(f[13]) + Number(f[14]);      // utime + stime, in clock ticks
  } catch { return null; }
}
async function cpuPercent(pid, ms) {
  const a = cpuTicks(pid);
  if (a === null) return null;
  await sleep(ms);
  const b = cpuTicks(pid);
  if (b === null) return null;
  return ((b - a) / 100) / (ms / 1000) * 100;   // USER_HZ is 100 on Linux
}

// One relay connection. Resolves with a tiny handle; every message is pushed to `seen` and
// `expect` waits for one by type with its own timeout — per-request, never per-connection,
// which is the failure mode the branch exists to remove.
function dial() {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(`${RELAY}/join/${ID}`);
    const waiters = [];
    const seen = [];
    const t = setTimeout(() => reject(new Error("relay did not accept the join in 10 s")), 10000);
    ws.onmessage = (ev) => {
      let m; try { m = JSON.parse(ev.data); } catch { return; }
      if (m.type === "ping") { ws.send(JSON.stringify({ type: "pong" })); return; }
      seen.push(m);
      for (let i = waiters.length - 1; i >= 0; i--) {
        if (waiters[i].types.includes(m.type)) { waiters[i].resolve(m); waiters.splice(i, 1); }
      }
      if (m.type === "join_accepted") { clearTimeout(t); resolve(handle); }
      if (m.type === "join_denied") { clearTimeout(t); reject(new Error("join denied: " + m.reason)); }
    };
    ws.onerror = () => { clearTimeout(t); reject(new Error("WebSocket error — is the relay up?")); };
    const handle = {
      send: (o) => ws.send(JSON.stringify(o)),
      close: () => ws.close(),
      mark: () => seen.length,
      // Terminate without a close frame, the way a phone going into a tunnel does.
      kill: () => { try { ws.close(3000, "yanked"); } catch {} },
      // `from` is the high-water mark: only messages that arrived *after* it count. Without it
      // a second request of the same kind resolves instantly against the first one's reply —
      // which made a resize report "0 ms" and the previous config's numbers.
      expect: (types, ms = 8000, from = 0) => new Promise((res, rej) => {
        const hit = seen.slice(from).find((m) => types.includes(m.type));
        if (hit) return res(hit);
        const w = { types, resolve: res };
        waiters.push(w);
        setTimeout(() => {
          const i = waiters.indexOf(w);
          if (i >= 0) { waiters.splice(i, 1); rej(new Error(`timed out waiting for ${types}`)); }
        }, ms);
      }),
    };
  });
}

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function main() {
  console.log(`graceful-probe → ${RELAY} id=${ID}\n`);

  // ── 1. start ──────────────────────────────────────────────────────────────
  let a = await dial();
  step(1, "joined; asking for a session");
  a.send({ type: "session_start", config: CONFIG });
  let m = await a.expect(["session_started", "session_alive", "session_error"], 15000);
  if (m.type === "session_alive") {
    step(1, "a session was already running — dropping it so the probe starts clean");
    a.send({ type: "session_stop" });
    await a.expect(["session_stopped"]);
    a.send({ type: "session_start", config: CONFIG });
    m = await a.expect(["session_started", "session_error"], 15000);
  }
  if (m.type !== "session_started") return fail(`session_start said ${m.type}: ${m.message || ""}`);
  step(1, `session started — encoder ${m.info?.encoder?.mode || "?"}`);

  // ── 2. launch ─────────────────────────────────────────────────────────────
  a.send({ type: "session_launch", command: LAUNCH });
  await a.expect(["session_launched"]);
  await sleep(SETTLE_MS);
  const before = markerPids();
  if (!before.length) return fail("the launched marker process never appeared — cannot measure survival");
  step(2, `launched ${LAUNCH.slice(0, 60)} — leader ${before[0]}, ${before.length} processes`);

  const pid = daemonPid();
  const busy = pid ? await cpuPercent(pid, 4000) : null;
  if (busy !== null) step(2, `daemon CPU while rendering for nobody: ${busy.toFixed(1)}% of a core`);

  // ── 3. yank the socket ────────────────────────────────────────────────────
  a.kill();
  step(3, `socket closed without a session_stop; waiting ${GAP_MS} ms`);
  await sleep(1000);                            // let the detach land before sampling
  const idle = pid ? await cpuPercent(pid, 4000) : null;
  if (idle !== null) {
    step(3, `daemon CPU while detached: ${idle.toFixed(1)}% of a core`
      + (busy !== null ? `  (was ${busy.toFixed(1)}%)` : ""));
    if (busy !== null && idle > busy) fail(`pausing the render tick cost CPU instead of saving it`);
  }
  await sleep(Math.max(0, GAP_MS - 5000));

  // ── 4. the measurement ────────────────────────────────────────────────────
  const during = markerPids();
  if (!survived(before, during)) {
    fail(`the applications did not survive the disconnect: [${before.join(",")}] → [${during.join(",")}]`);
  } else {
    step(4, `applications survived — pid ${before[0]} alive, ${during.length}/${before.length} processes`);
  }

  const b = await dial();
  b.send({ type: "session_start", config: CONFIG });
  const back = await b.expect(["session_alive", "session_started", "session_error"], 15000);
  if (back.type !== "session_alive") {
    fail(`after reconnecting the daemon said ${back.type} — the session did not survive`);
  } else {
    step(4, `the daemon still has the session: ${back.info?.encoder?.mode || "?"}`);
  }

  // ── 5-6. rejoin, and check nothing was recycled underneath ────────────────
  b.send({ type: "session_rejoin" });
  const re = await b.expect(["session_started", "session_error"], 10000);
  if (re.type !== "session_started") fail(`session_rejoin said ${re.type}: ${re.message || ""}`);
  else step(5, "rejoined the running session");

  const after = markerPids();
  if (!survived(before, after)) fail(`the applications did not survive the rejoin: [${before.join(",")}] → [${after.join(",")}]`);
  else step(6, `same applications after the rejoin — pid ${before[0]} alive`);

  // ── 7. change the session while it runs ───────────────────────────────────
  //
  // Two shapes of change, because they exercise different code and one of them has already
  // killed an application here. A bitrate change touches the encoder only; a resize also
  // replaces the `Output`, and replacing an output that a client has bound is how a session
  // lost its kitty on 2026-09-13 02:55:49 (`windows=1` on the reconfigure line, `windows=0` on
  // the next line of the same session).
  //
  // The measurement is never "was the message answered". It is "are the same pids still there".
  async function reconfigure(label, cfg, budgetMs) {
    const t0 = Date.now();
    const mark = b.mark();
    b.send({ type: "session_reconfigure", config: cfg });
    const rc = await b.expect(["session_reconfigured", "session_error"], 15000, mark);
    const took = Date.now() - t0;
    if (rc.type !== "session_reconfigured") {
      fail(`${label}: reconfigure said ${rc.type}: ${rc.message || ""}`);
      return;
    }
    const e = rc.info?.encoder || {};
    step(7, `${label} in ${took} ms — ${e.fps} fps, ${e.bitrate_kbps} kbps, ${e.mode}`);
    if (cfg.fps !== e.fps) fail(`${label}: asked ${cfg.fps} fps, got ${e.fps}`);
    const wantKbps = cfg.quality?.custom?.bitrate_kbps;
    if (wantKbps && wantKbps !== e.bitrate_kbps) {
      fail(`${label}: asked ${wantKbps} kbps, got ${e.bitrate_kbps}`);
    }
    if (took > budgetMs) fail(`${label}: took ${took} ms, over the ${budgetMs} ms budget`);
    // The window the application had is the thing at risk, so give it time to die if it is
    // going to. 500 ms was enough to catch the output-global bug.
    await sleep(1500);
    const now = markerPids();
    if (!survived(before, now)) {
      fail(`${label}: the applications did not survive — [${before.join(",")}] → [${now.join(",")}]`);
    } else {
      step(7, `${label}: applications survived — pid ${before[0]} alive, ${now.length} processes`);
    }
  }

  // Same geometry, new bitrate and nothing else. The output must not be rebuilt at all.
  await reconfigure(
    "bitrate only",
    { width: 1280, height: 720, fps: 60, scale: 1.0, quality: { custom: { bitrate_kbps: 2500 } } },
    1000,
  );
  // A real resize: new resolution, new aspect ratio, new frame rate. This is the one that
  // replaces the `Output`.
  await reconfigure(
    "resize 1280x720 -> 960x540@30",
    { width: 960, height: 540, fps: 30, scale: 1.0, quality: { custom: { bitrate_kbps: 1800 } } },
    1000,
  );

  // A configuration that cannot work. Two flavours, and the daemon must answer both rather
  // than going quiet — and must still be running a session afterwards.
  async function refused(label, cfg) {
    const mark = b.mark();
    b.send({ type: "session_reconfigure", config: cfg });
    let rc;
    try {
      rc = await b.expect(["session_reconfigured", "session_error"], 8000, mark);
    } catch (_) {
      fail(`${label}: the daemon said nothing at all`);
      return;
    }
    if (rc.type !== "session_error") { fail(`${label}: was accepted (${rc.type})`); return; }
    step(7, `${label}: refused — "${(rc.message || "").slice(0, 70)}"`);
    await sleep(800);
    if (!survived(before, markerPids())) fail(`${label}: the applications died anyway`);
  }

  // Rejected before anything is built: odd width has no valid 4:2:0 chroma plane.
  await refused("odd width 1281", { width: 1281, height: 720, fps: 60, scale: 1.0, quality: "balanced" });
  await refused("zero fps", { width: 1280, height: 720, fps: 0, scale: 1.0, quality: "balanced" });
  await refused("absurd scale", { width: 1280, height: 720, fps: 60, scale: 99, quality: "balanced" });

  // An extreme but *valid* size. Whether the encoder opens at 8K is a property of the hardware,
  // not of this code — on the machine this was written on, VAAPI opens it. So the assertion is
  // not "it fails"; it is that **either answer leaves a working session**. That is the invariant
  // the reconfigure path has to hold: it releases the old encoder before building the new one,
  // so a failure in between leaves a live session with no pipeline.
  {
    const mark = b.mark();
    b.send({ type: "session_reconfigure", config: { width: 7680, height: 4320, fps: 60, scale: 1.0, quality: "balanced" } });
    const rc = await b.expect(["session_reconfigured", "session_error"], 20000, mark);
    step(7, `8K: ${rc.type === "session_error" ? "refused — " + (rc.message || "") : "accepted by this hardware"}`);
    await sleep(1000);
    if (!survived(before, markerPids())) fail("8K: the applications died");
  }

  // And the session must still work after both refusals.
  await reconfigure(
    "recovery after the extremes",
    { width: 1280, height: 720, fps: 60, scale: 1.0, quality: { custom: { bitrate_kbps: 4000 } } },
    2000,
  );

  // ── 8. clean up ───────────────────────────────────────────────────────────
  b.send({ type: "session_stop" });
  await b.expect(["session_stopped"]);
  await sleep(1000);
  const gone = markerPids();
  if (gone.length) fail(`session_stop left ${gone.length} process(es) behind: [${gone.join(",")}]`);
  else step(8, "an explicit stop still kills everything — no leak");
  b.close();
}

main()
  .catch((e) => fail(e.message))
  .finally(() => {
    console.log(failed ? `\nFAIL — ${failed}` : "\nPASS — the session outlives its viewer");
    process.exit(failed ? 1 : 0);
  });
