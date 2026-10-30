// P1's first budget numbers on the emulator (docs/superpowers/plans/2026-10-30-native-android.md,
// A8): cold start → home's first frame (`am start -W` TotalTime), and cold start → live link words
// (the core's own `budget:` log line, process start to the first `Link{open}`). A release daemon and
// a local relay stand in for the computer; the emulator reaches them through `adb reverse`. The app pairs
// natively from a `wado://pair` link first.
//
//   cargo build --release -p wado -p wado-relay
//   nix develop .#android-emulator -c node scripts/android-budget.mjs [runs]   (sandbox off)
//
// Emulator numbers are recorded only: budgets are judged on a phone. Kills only its own processes.

import { spawn, execFileSync } from "node:child_process";
import { mkdtempSync, mkdirSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const BIN = join(ROOT, process.env.WADO_BIN || "target/release");
const APK = join(ROOT, "android/app/build/outputs/apk/debug/app-debug.apk");
const PKG = "io.github.sandptel.wado.debug";
const PORT = 4993;
const RID = "999000444";
const RUNS = Number(process.argv[2] || 5);
const T = mkdtempSync(join(tmpdir(), "wado-budget-"));
const CFG = join(T, "cfg");
const RUN = join(T, "run");
mkdirSync(join(CFG, "wado"), { recursive: true });
mkdirSync(RUN, { recursive: true, mode: 0o700 });
const ENV = { ...process.env, XDG_CONFIG_HOME: CFG, XDG_RUNTIME_DIR: RUN };
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = new Set();
const adb = (...a) => execFileSync("adb", a).toString();

function start(bin, args, env = {}) {
  const p = spawn(bin, args, { env: { ...ENV, ...env }, stdio: ["ignore", "pipe", "pipe"] });
  p.out = "";
  const take = (d) => { p.out += d.toString().replace(/\x1b\[[0-9;]*m/g, ""); };
  p.stdout.on("data", take);
  p.stderr.on("data", take);
  procs.add(p);
  p.on("exit", () => procs.delete(p));
  return p;
}
const median = (a) => { const s = [...a].sort((x, y) => x - y); return s[Math.floor(s.length / 2)]; };

let code = 1;
try {
  start(join(BIN, "wado-relay"), ["--bind", "127.0.0.1:" + PORT]);
  await sleep(400);
  const d = start(join(BIN, "wado"), ["daemon"], { WADO_RELAY_URL: `ws://127.0.0.1:${PORT}`, WADO_REMOTE_ID: RID, WADO_INSTANCE: "1", RUST_LOG: "info" });
  for (let i = 0; i < 150 && !/registered — Remote ID/.test(d.out); i++) await sleep(100);
  const out = execFileSync(join(BIN, "wado"), ["qr", "--relay", `http://127.0.0.1:${PORT}`, "--id", RID], { env: ENV }).toString().replace(/\x1b\[[0-9;]*m/g, "");
  const link = out.match(/open: (\S+)/)[1];
  const q = link.slice(link.indexOf("?"));
  adb("install", "-r", "-g", APK);
  // The emulator's own 127.0.0.1:PORT reaches the host's (10.0.2.2 did not answer here).
  adb("reverse", `tcp:${PORT}`, `tcp:${PORT}`);
  adb("shell", "pm", "clear", PKG);
  adb("logcat", "-c");
  // Pair natively, from a wado:// link, on a cold start.
  adb("shell", "am", "start", "-W", "-a", "android.intent.action.VIEW", "-d", `'wado://pair${q}'`, "-n", `${PKG}/io.github.sandptel.wado.MainActivity`);
  let paired = false;
  for (let i = 0; i < 40 && !paired; i++) {
    await sleep(500);
    try { paired = readFileSync(join(CFG, "wado/trusted_clients"), "utf8").includes("wado app"); } catch {}
  }
  console.log("paired natively:", paired);
  if (!paired) console.log(adb("logcat", "-d", "-s", "wado-core:*", "AndroidRuntime:E").slice(-3000));
  const starts = [], live = [];
  for (let r = 0; r < RUNS; r++) {
    adb("shell", "am", "force-stop", PKG);
    adb("logcat", "-c");
    await sleep(800);
    const w = adb("shell", "am", "start", "-W", "-n", `${PKG}/io.github.sandptel.wado.MainActivity`);
    const t = Number((w.match(/TotalTime: (\d+)/) || [])[1]);
    if (t) starts.push(t);
    let ms = null;
    for (let i = 0; i < 20 && ms == null; i++) {
      await sleep(500);
      const log = adb("logcat", "-d", "-s", "wado-core:I");
      const m = log.match(/live link words in (\d+) ms/);
      if (m) ms = Number(m[1]);
    }
    if (ms != null) live.push(ms);
    console.log(`run ${r + 1}: first frame ${t} ms, live link words ${ms ?? "—"} ms`);
  }
  console.log(`cold start → first frame (TotalTime): p50 ${median(starts)} ms over ${starts.length}`);
  console.log(`cold start → live link words: p50 ${live.length ? median(live) : "—"} ms over ${live.length}`);
  code = paired && live.length ? 0 : 1;
} catch (e) {
  console.error(e.message || e);
} finally {
  for (const p of procs) try { p.kill("SIGTERM"); } catch {}
  await sleep(300);
  process.exit(code);
}
