// End-to-end checks that need a real session: the clipboard both ways, and notifications.
//
//   node scripts/session-e2e.mjs [target/debug]     (default target/release; needs a GPU and
//                                                      wl-clipboard, and the sandbox off)
//
// Starts a small session, then uses a shell inside it (which inherits WAYLAND_DISPLAY):
//
//   session → phone   `wl-copy` in the session reaches the viewer as a `clipboard` message
//   phone → session   text the viewer sends is what `wl-paste` reads in the session
//   notifications     `notify-send`, launched as a session app (so on the session's own bus),
//                     reaches the viewer
//   workspaces        a window opens on 1, moves to 2, 2 shows it, Home finds an empty one
//   sessions          listed; left running (detach); found and resumed from another device,
//                     re-fitted to its shape; gone from the list once ended

import { spawn } from "node:child_process";
import { mkdtempSync, mkdirSync, writeFileSync, readFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const BIN = join(ROOT, process.argv[2] || "target/release");
const PORT = 4989;
const RELAY = `ws://127.0.0.1:${PORT}`;
const RID = "999000444";
const HOME = mkdtempSync(join(tmpdir(), "wado-clip-e2e-"));
mkdirSync(join(HOME, ".ssh"));
writeFileSync(join(HOME, ".ssh/config"), "Host fakebox\n  HostName 192.0.2.1\nHost *.lan\n");
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = [];
let failures = 0;
const check = (name, ok, detail = "") => {
  if (!ok) failures++;
  console.log(`${ok ? "  ok  " : "FAIL  "}${name}${ok || !detail ? "" : "\n        " + detail}`);
};
const start = (bin, args, env) => {
  const p = spawn(join(BIN, bin), args, { env: { ...process.env, ...env }, stdio: ["ignore", "pipe", "pipe"] });
  p.out = "";
  p.stdout.on("data", (d) => (p.out += d));
  p.stderr.on("data", (d) => (p.out += d));
  procs.push(p);
  return p;
};

class Client {
  constructor(name, takeover = false) {
    this.msgs = [];
    const q = new URLSearchParams({ client: name + "-key", name });
    if (takeover) q.set("takeover", "1");
    this.ws = new WebSocket(`${RELAY}/join/${RID}?${q}`);
    this.ws.onmessage = (e) => {
      const m = JSON.parse(e.data);
      if (m.type === "ping") return this.send({ type: "pong" });
      this.msgs.push(m);
    };
  }
  send(m) { this.ws.send(JSON.stringify(m)); }
  async waitWhere(pred, ms = 10000) {
    const t0 = Date.now();
    while (Date.now() - t0 < ms) {
      const m = this.msgs.find(pred);
      if (m) return m;
      await sleep(50);
    }
    return null;
  }
  wait(type, ms) { return this.waitWhere((m) => m.type === type, ms); }
  output(id) { return this.msgs.filter((m) => m.type === "pty_output" && m.id === id).map((m) => m.data).join(""); }
  close() { this.ws.close(); }
}

// Both devices already trusted — as after an approval — so the run tests sessions, not the gate.
mkdirSync(join(HOME, ".config/wado"), { recursive: true });
writeFileSync(join(HOME, ".config/wado/trusted_clients"), "Phone-key\tPhone\nLaptop-key\tLaptop\n");

try {
  start("wado-relay", ["--bind", `127.0.0.1:${PORT}`], {});
  await sleep(400);
  const d = start("wado", [], {
    WADO_RELAY_URL: RELAY, WADO_REMOTE_ID: RID, WADO_INSTANCE: "e2e-session", WADO_UDP_SLICE: "22",
    HOME, XDG_CONFIG_HOME: join(HOME, ".config"), SHELL: "/bin/sh",
  });
  const a = new Client("Phone");
  check("joined", !!(await a.wait("join_accepted", 20000)));
  a.send({ type: "session_start", config: { width: 640, height: 360, fps: 30, quality: "balanced" } });
  const started = await a.waitWhere((m) => m.type === "session_started" || m.type === "session_error", 30000);
  check("a session starts", started && started.type === "session_started", JSON.stringify(started) + d.out.slice(-400));
  a.send({ type: "pty_open", cols: 120, rows: 30 });
  const { id } = (await a.wait("pty_opened")) || {};

  console.log("session → phone");
  a.send({ type: "pty_input", id, data: "wl-copy from-the-session\n" });
  const got = await a.waitWhere((m) => m.type === "clipboard", 10000);
  check("an app's copy reaches the viewer", got && got.text === "from-the-session", JSON.stringify(got) + "\n" + d.out.split("\n").filter((l) => /clipboard/i.test(l)).join("\n"));

  console.log("phone → session");
  a.send({ type: "clipboard_set", text: "from-the-phone" });
  await sleep(300);
  a.send({ type: "pty_input", id, data: "echo pasted:$(wl-paste --no-newline)\n" });
  check("the viewer's text is what the session pastes",
    !!(await a.waitWhere(() => a.output(id).includes("pasted:from-the-phone"), 10000)), a.output(id).slice(-300));

  console.log("notifications");
  a.send({ type: "session_launch", command: "notify-send -a e2e 'Build done' 'all green'" });
  const n = await a.waitWhere((m) => m.type === "notification", 15000);
  check("an app's notification reaches the viewer", n && n.summary === "Build done" && n.body === "all green" && n.app === "e2e",
    JSON.stringify(n) + "\n" + d.out.split("\n").filter((l) => /notif|bus/i.test(l)).slice(-5).join("\n"));

  console.log("workspaces");
  a.send({ type: "session_launch", command: "gnome-calculator" });
  const w1 = await a.waitWhere((m) => m.type === "windows" && m.windows.length > 0, 15000);
  check("a new window opens on workspace 1", w1 && w1.workspace === 1 && w1.windows[0].workspace === 1, JSON.stringify(w1));
  const wid = w1 && w1.windows[0].id;
  a.msgs.length = 0;
  a.send({ type: "session_window", action: { move_to_workspace: { id: wid, n: 2, follow: false } } });
  const w2 = await a.waitWhere((m) => m.type === "windows" && m.windows.some((w) => w.id === wid && w.workspace === 2), 5000);
  check("moving it to workspace 2 files it there, still listed", !!w2 && w2.workspace === 1, JSON.stringify(a.msgs.filter((m) => m.type === "windows").slice(-1)));
  a.msgs.length = 0;
  a.send({ type: "session_window", action: { workspace: { n: 2 } } });
  check("switching to workspace 2 shows it", !!(await a.waitWhere((m) => m.type === "windows" && m.workspace === 2, 5000)));
  a.msgs.length = 0;
  a.send({ type: "session_window", action: "home" });
  const home = await a.waitWhere((m) => m.type === "windows" && m.workspace !== 2, 5000);
  check("Home goes to an empty workspace", home && !home.windows.some((w) => w.workspace === home.workspace), JSON.stringify(home));

  console.log("sessions");
  await sleep(3500); // one directory heartbeat
  a.msgs.length = 0;
  a.send({ type: "sessions_request" });
  const l1 = await a.wait("sessions");
  check("the running session is listed, with its apps", l1 && l1.sessions.length === 1 && l1.sessions[0].width === 640, JSON.stringify(l1));
  a.send({ type: "session_detach" });
  a.close();
  await sleep(4000);
  const b = new Client("Laptop", true);
  check("another device joins", !!(await b.wait("join_accepted", 20000)));
  b.send({ type: "sessions_request" });
  const l2 = await b.wait("sessions");
  check("…sees the session left running", l2 && l2.sessions.length === 1 && l2.sessions[0].detached === true, JSON.stringify(l2));
  b.send({ type: "session_rejoin" });
  check("…and resumes it", !!(await b.waitWhere((m) => m.type === "session_started", 10000)));
  b.send({ type: "session_reconfigure", config: { width: 360, height: 640, fps: 30, quality: "balanced" } });
  check("…re-fitted to its own shape, apps kept", !!(await b.waitWhere((m) => m.type === "session_reconfigured", 10000)));
  b.send({ type: "session_stop" });
  await sleep(4000);
  b.msgs.length = 0;
  b.send({ type: "sessions_request" });
  const l3 = await b.wait("sessions");
  check("ending it removes it from the list", l3 && l3.sessions.length === 0, JSON.stringify(l3));
} finally {
  for (const p of procs) p.kill("SIGTERM");
}
console.log(failures ? `\n${failures} failed` : "\nall passed");
process.exit(failures ? 1 : 0);
