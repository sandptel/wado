// Perf report: one comparable row per test run, from the rig's own logs.
//
//   node scripts/perf-report.mjs [--since HH:MM[:SS]] [--until HH:MM[:SS]] [--label NAME] [--save]
//
// Default window: the last 10 minutes. `--save` appends the result to plan/data/perf-runs.jsonl
// (local, gitignored) so runs can be compared across phases of the latency work
// (Decision Log 2026-10-03, "latency compass").
//
// THE PROTOCOL — keep every run comparable:
//   1. `scripts/rig.sh` (release build, always), phone on the link under test, latency readout on.
//      Record which Display & stream switches were on (low-latency audio, auto bitrate) in the label.
//   2. Note the clock, then do the scenario, about a minute each:
//        idle desktop · scroll a long page · drag a window · play a video with sound
//   3. `node scripts/perf-report.mjs --since <clock> --label <phase>-<what changed> --save`
//   Same phone, same network type, same resolution/fps for an A/B. Write the network type into
//   the label (wifi / 4g / 5g). Compare rows, not impressions.
//
// What it reads (all logged already): the browser's 5 s `stats` lines (fps, rtt, jbuf, jtarget,
// dec, kbps, drops), its `latency` legs (capture/encode/queue/net/buf/decode/input), the pump's
// write_sample distribution, shedding, auto-bitrate steps, verdicts by side, and the session's
// shape. Samples are one per 5 s slot, normal or ANOMALY, so a bad stretch is neither
// over-weighted nor dropped.
import { readFileSync, readdirSync, appendFileSync, mkdirSync } from "node:fs";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const LOGS = process.env.WADO_RIG_LOGS || "/tmp/wado-rig";
const arg = (k) => { const i = process.argv.indexOf(k); return i > 0 ? process.argv[i + 1] : undefined; };
const label = arg("--label") || "unlabelled";
const save = process.argv.includes("--save");

const lines = readdirSync(LOGS).filter((f) => /^daemon-\d+\.log(\.\d)?$/.test(f))
  .flatMap((f) => readFileSync(join(LOGS, f), "utf8").replace(/\x1b\[[0-9;]*m/g, "").split("\n"))
  .filter((l) => /^\d{4}-\d\d-\d\dT/.test(l)).sort();
if (!lines.length) { console.error("no rig logs in " + LOGS); process.exit(1); }
const day = lines.at(-1).slice(0, 10);
// HH:MM[:SS] is local time like the rig's banner; a trailing Z means UTC, like the logs.
const at = (hm) => {
  if (!hm) return null;
  const z = hm.endsWith("Z"), t = z ? hm.slice(0, -1) : hm;
  return new Date(`${day}T${t.length === 5 ? t + ":00" : t}${z ? "Z" : ""}`);
};
const last = new Date(lines.at(-1).slice(0, 23) + "Z");
// Local clock in, like the rig's own banner; logs are UTC.
const since = at(arg("--since")) || new Date(last - 10 * 60e3);
const until = at(arg("--until")) || last;
const inWin = lines.filter((l) => { const t = new Date(l.slice(0, 23) + "Z"); return t >= since && t <= until; });

const kv = (l) => Object.fromEntries([...l.matchAll(/([a-z_()]+)[=~]"?(-?[\d.]+)/gi)].map((m) => [m[1], Number(m[2])]));
const pct = (xs, p) => { if (!xs.length) return null; const s = [...xs].sort((a, b) => a - b); return s[Math.min(s.length - 1, Math.floor(p * s.length))]; };
const dist = (xs) => ({ n: xs.length, p50: pct(xs, 0.5), p90: pct(xs, 0.9), max: xs.length ? Math.max(...xs) : null });
const col = (rows, k) => rows.map((r) => r[k]).filter((v) => Number.isFinite(v) && v >= 0);

// One sample per 5 s slot, normal or ANOMALY: ANOMALY lines come every second while things are
// bad and `stats` lines every fifth second otherwise, so taking all of them over-weights bad
// stretches and taking only `stats` empties a run that was bad throughout (2 samples of 7 min,
// measured). The last sample in each slot stands for it.
const slots = new Map();
for (const l of inWin.filter((l) => l.includes("browser: stats ") || l.includes("browser: ANOMALY "))) {
  slots.set(Math.floor(new Date(l.slice(0, 23) + "Z") / 5000), l);
}
const stats = [...slots.values()].map(kv);
const lat = inWin.filter((l) => l.includes("browser: latency ")).map(kv);
const ws = inWin.filter((l) => l.includes("write_sample distribution")).map(kv);
const drops = col(stats, "framesDropped"), recv = col(stats, "framesReceived");
const verdicts = {};
for (const l of inWin.filter((l) => l.includes("browser: verdict "))) {
  const m = l.match(/verdict (\w+) ([a-z ]+?)(?: —|  |$)/);
  if (m) verdicts[`${m[1]} ${m[2].trim()}`] = (verdicts[`${m[1]} ${m[2].trim()}`] || 0) + 1;
}
const shapes = [...new Set(inWin.flatMap((l) => {
  const a = l.match(/compositor session active width=(\d+) height=(\d+) fps=(\d+) bitrate_kbps=(\d+)/);
  if (a) return [`${a[1]}x${a[2]}@${a[3]} ${a[4]}kbps`];
  const r = l.match(/session reconfigured .* to=(\S+) (\d+kbps)/);
  return r ? [`${r[1]} ${r[2]}`] : [];
}))];

const report = {
  label, day, since: since.toISOString(), until: until.toISOString(),
  shapes,
  client: {
    fps: dist(col(stats, "fps")), rtt_ms: dist(col(stats, "rtt")), jbuf_ms: dist(col(stats, "jbuf")),
    jtarget_ms: dist(col(stats, "jtarget")), decode_ms: dist(col(stats, "dec")), kbps: dist(col(stats, "kbps")),
    audio_buf_ms: dist(col(stats, "abuf")), audio_target_ms: dist(col(stats, "atarget")),
    sync_hold_ms: dist(col(stats, "vmin")), input_rt_ms: dist(col(stats, "input")),
    frames_dropped: drops.length ? Math.max(...drops) - Math.min(...drops) : null,
    frames_received: recv.length ? Math.max(...recv) - Math.min(...recv) : null,
    anomalies: inWin.filter((l) => l.includes("browser: ANOMALY")).length,
  },
  legs_ms: Object.fromEntries(["capture", "encode", "queue", "net", "buf", "decode", "input(rt)"]
    .map((k) => [k, pct(col(lat, k), 0.5)])),
  server: {
    write_sample_p99_ms: dist(col(ws, "p99_ms")).max,
    over_budget: col(ws, "over_budget").reduce((a, b) => a + b, 0),
    shedding_events: inWin.filter((l) => l.includes("shedding render ticks")).length,
  },
  autorate_steps: inWin.filter((l) => l.includes("browser: autorate")).length,
  verdicts,
  crashes: inWin.filter((l) => l.includes("UI crashed")).length,
};

const f = (d) => (d && d.p50 != null ? `${d.p50}/${d.p90}/${d.max}` : "—");
console.log(`\n${label}  ${report.since.slice(11, 19)}–${report.until.slice(11, 19)} UTC  ${shapes.join(" → ") || "(no session start in window)"}`);
console.log(`  samples ${stats.length} (one per 5 s), anomaly lines ${report.client.anomalies}, crashes ${report.crashes}`);
console.log("  p50/p90/max   fps " + f(report.client.fps) + "  rtt " + f(report.client.rtt_ms) + "  jbuf " + f(report.client.jbuf_ms) +
  "  jtarget " + f(report.client.jtarget_ms) + "  decode " + f(report.client.decode_ms) + "  kbps " + f(report.client.kbps));
console.log("  audio/sync    abuf " + f(report.client.audio_buf_ms) + "  atarget " + f(report.client.audio_target_ms) +
  "  sync hold " + f(report.client.sync_hold_ms) + "  input rt " + f(report.client.input_rt_ms));
console.log(`  frames dropped ${report.client.frames_dropped ?? "—"} of ${report.client.frames_received ?? "—"} received`);
console.log("  legs p50 (ms)  " + Object.entries(report.legs_ms).map(([k, v]) => `${k} ${v ?? "—"}`).join("  "));
console.log(`  server: write_sample p99 max ${report.server.write_sample_p99_ms ?? "—"} ms, over budget ${report.server.over_budget}, shedding ${report.server.shedding_events}`);
console.log(`  auto bitrate steps ${report.autorate_steps}   verdicts ${JSON.stringify(verdicts)}`);
if (save) {
  mkdirSync(join(ROOT, "plan/data"), { recursive: true });
  appendFileSync(join(ROOT, "plan/data/perf-runs.jsonl"), JSON.stringify(report) + "\n");
  console.log("  saved → plan/data/perf-runs.jsonl");
}
