// The Pocket Console in a real browser, with no computer behind the page (`?fixture`): a canvas
// stream plays the session's picture, and the daemon's side is played through `W.fixture`.
//
//   dx build -p wado-client --platform web
//   node scripts/console-check.mjs [out-dir]      (needs Chrome; run with the sandbox off)
//
// Checks, per form (390×844, 844×390, 820×1180, 1280×800):
//   • layout slots: the picture is inset at the session's aspect, no control overlaps it, every
//     control is on screen and at least 44 px (the click zones: 30 px tall, ≥ 180 px wide)
//   • input mapping against the *transformed* video rect: the picture's corners and centre map to
//     0/0.5/1 in the console, in the cockpit, and mid-morph (animations frozen at t = 0.2)
//   • the toggle: Full screen ⇄ Leave full screen, aria-pressed, the picture filling the display,
//     focus kept on the same node, and back
// Screenshots of each form in default-dark and catppuccin-latte, plus the faces, go to out-dir.

import { spawn } from "node:child_process";
import { mkdirSync, mkdtempSync, writeFileSync } from "node:fs";
import { tmpdir } from "node:os";
import { join } from "node:path";

const ROOT = new URL("..", import.meta.url).pathname;
const PUB = process.env.WADO_PUB || join(ROOT, "target/dx/wado-client/debug/web/public");
const OUT = process.argv[2] || join(tmpdir(), "wado-console-shots");
mkdirSync(OUT, { recursive: true });
const T = mkdtempSync(join(tmpdir(), "wado-console-"));
const HTTP = 8773, CDP = 9373;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
const procs = [];
let failures = 0;
const ok = (c, name, d = "") => { if (!c) failures++; console.log(`${c ? "  ok  " : "FAIL  "}${name}${c || !d ? "" : "\n        " + d}`); };
const start = (bin, args) => { const p = spawn(bin, args, { stdio: "ignore" }); procs.push(p); return p; };
const cleanup = () => { for (const p of procs) try { p.kill("SIGTERM"); } catch {} };
for (const sig of ["SIGTERM", "SIGINT"]) process.on(sig, () => { cleanup(); process.exit(1); });

start("python3", ["-m", "http.server", String(HTTP), "--bind", "127.0.0.1", "-d", PUB]);
start("google-chrome-stable", ["--headless=new", "--remote-debugging-port=" + CDP, "--user-data-dir=" + join(T, "chrome"),
  "--no-first-run", "--hide-scrollbars", "--autoplay-policy=no-user-gesture-required", "about:blank"]);
let list;
for (let i = 0; i < 60 && !list; i++) { await sleep(200); try { list = await (await fetch(`http://127.0.0.1:${CDP}/json/list`)).json(); } catch {} }
const ws = new WebSocket(list.find((t) => t.type === "page").webSocketDebuggerUrl);
await new Promise((r) => (ws.onopen = r));
let id = 0; const pend = new Map();
ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); } };
const cdp = (method, params = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
const js = async (expr) => {
  const r = await cdp("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
  if (r.result?.exceptionDetails) throw new Error(r.result.exceptionDetails.exception?.description || "evaluate failed");
  return r.result?.result?.value;
};
const until = async (expr, ms = 8000) => { const t = Date.now(); while (Date.now() - t < ms) { if (await js(expr).catch(() => false)) return true; await sleep(120); } return false; };
const finish = () => js(`document.getAnimations().forEach(a => { try { a.finish(); } catch {} }); true`);
const shot = async (name) => {
  await sleep(500);
  await finish();
  await sleep(120);
  const r = await cdp("Page.captureScreenshot", { format: "png" });
  writeFileSync(join(OUT, name + ".png"), Buffer.from(r.result.data, "base64"));
};

await cdp("Runtime.enable");
await cdp("Page.enable");
const BASE = `http://127.0.0.1:${HTTP}/`;

// A stand-in for the session's picture: a desktop drawn on a canvas, streamed into #wado-video.
const PICTURE = `(() => {
  const v = document.getElementById("wado-video");
  if (!v) return false;
  const W0 = Math.round(innerWidth * 2), H0 = Math.round(innerHeight * 2);
  const c = document.createElement("canvas"); c.width = W0; c.height = H0;
  const g = c.getContext("2d");
  const draw = () => {
    g.fillStyle = "#2b3a4a"; g.fillRect(0, 0, W0, H0);
    g.fillStyle = "#f4f1ea"; g.fillRect(W0 * .04, H0 * .05, W0 * .92, H0 * .86);
    g.fillStyle = "#d9622b"; g.fillRect(W0 * .04, H0 * .05, W0 * .92, H0 * .04);
    g.fillStyle = "#333"; g.font = (W0 / 22) + "px sans-serif";
    g.fillText("Repotting a monstera", W0 * .08, H0 * .16);
    g.fillStyle = "#9aa"; for (let i = 0; i < 9; i++) g.fillRect(W0 * .08, H0 * (.22 + i * .05), W0 * (.6 + (i % 3) * .1), H0 * .018);
    g.fillStyle = "#1e2630"; g.fillRect(0, H0 * .93, W0, H0 * .07);
  };
  draw();
  setInterval(draw, 200);
  v.srcObject = c.captureStream(15);
  v.muted = true; v.play().catch(() => {});
  return true;
})()`;

const WINDOWS = { type: "windows", workspace: 1, windows: [
  { id: 1, title: "Repotting a monstera", app_id: "firefox", focused: true, workspace: 1 },
  { id: 2, title: "~/src/wado — cargo test", app_id: "foot", focused: false, workspace: 1 },
  { id: 3, title: "Downloads", app_id: "org.gnome.Nautilus", focused: false, workspace: 2 },
] };

async function open(w, h, opts = {}) {
  const mobile = opts.mobile !== false;
  await cdp("Emulation.setDeviceMetricsOverride", { width: w, height: h, deviceScaleFactor: 2, mobile });
  await cdp("Emulation.setTouchEmulationEnabled", { enabled: mobile, maxTouchPoints: mobile ? 5 : 1 });
  await cdp("Emulation.setEmulatedMedia", { features: [{ name: "prefers-reduced-motion", value: opts.reduced ? "reduce" : "no-preference" }] });
  const settings = JSON.stringify({ conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: "123456789", profile: 0,
    theme: opts.theme || "default-dark",
    profiles: [{ name: "Framework", host: "", conn_mode: "relay", relay_url: "http://10.255.255.1:9", remote_id: "123456789" }] });
  await cdp("Page.navigate", { url: BASE });
  await sleep(500);
  await js(`localStorage.clear(); localStorage.setItem("wado.settings", ${JSON.stringify(settings)});
    ${opts.face ? `localStorage.setItem("wado.console", JSON.stringify({open: "console", face: ${JSON.stringify(opts.face)}}));` : ""} true`);
  await cdp("Page.navigate", { url: BASE + "?fixture" });
  await until("!!window.__wado && !!window.__wado.fixture", 10000);
  await js(`(() => { const W = window.__wado;
    W.fixture({ type: "sessionOn" });
    W.fixture(${JSON.stringify(WINDOWS)});
    W.fixture({ type: "encoder", mode: ${JSON.stringify(opts.sw ? "software" : "hardware")}, pipeline: ${JSON.stringify(opts.sw ? "x264-cpu" : "vaapi-dmabuf")} });
    W.fixture({ type: "link", state: "direct", path: "lan", rtt_ms: 23, net: "wifi" });
    return true; })()`);
  ok(await until("document.documentElement.dataset.arr === 'console' && !!document.getElementById('wado-video')", 8000), `${w}×${h}: the session opens as a console`);
  await js(PICTURE);
  ok(await until("document.getElementById('wado-video').videoWidth > 0", 6000), `${w}×${h}: the picture plays`);
  await until("!!window.__wado.stageFit.layout && window.__wado.stageFit.layout.w === innerWidth", 4000);
  await js(`window.__wado.fixturePresence && window.__wado.fixturePresence(
    [{id:7,name:"Ana's iPad",hue:6,x:.64,y:.41,kind:"pointer"}],
    [{id:3,name:"Pixel 8",hue:3,lead:true},{id:7,name:"Ana's iPad",hue:6},{id:9,name:"ThinkPad",hue:4}], 3); true`);
  await sleep(700);
  await finish();
}

// The picture's content rect (object-fit: contain) from the video's transformed box — exactly
// what input mapping uses — and normPoint at its corners and centre.
const MAP = `(() => {
  const W = window.__wado, v = document.getElementById("wado-video");
  const c = W.targets.content(v);
  const at = (fx, fy) => W.normPoint(c.left + fx * c.w, c.top + fy * c.h, v);
  const pts = [[.5, .5], [.02, .02], [.98, .98], [.25, .75]].map(([x, y]) => ({ want: [x, y], got: at(x, y) }));
  const err = Math.max(...pts.map((p) => Math.max(Math.abs(p.got.x - p.want[0]), Math.abs(p.got.y - p.want[1]))));
  return { c, err, k: W.cssToLogical(v) };
})()`;

const CONTROLS = `[...document.querySelectorAll(
  "#wado-bar .barbtn, .rail .barbtn, #wado-bar .wsnum, #wado-bar .wsapp, .cstrip button, .cfaces button, .linkbadge .linkpill, .cdeck button")]
  .filter((el) => !el.closest("[inert]") && getComputedStyle(el).visibility !== "hidden" && +getComputedStyle(el).opacity > 0.01 && el.getBoundingClientRect().width > 0)`;

async function checkLayout(tag) {
  const r = await js(`(() => {
    const W = window.__wado, v = document.getElementById("wado-video");
    const c = W.targets.content(v);
    const pic = { x: c.left, y: c.top, w: c.w, h: c.h };
    const vw = v.videoWidth, vh = v.videoHeight;
    const els = ${CONTROLS};
    const bad = [], small = [], off = [];
    for (const el of els) {
      const b = el.getBoundingClientRect();
      const name = (el.getAttribute("aria-label") || el.className || el.tagName).slice(0, 40);
      if (b.left < pic.x + pic.w - 1 && pic.x < b.right - 1 && b.top < pic.y + pic.h - 1 && pic.y < b.bottom - 1) bad.push(name);
      if (b.left < -1 || b.top < -1 || b.right > innerWidth + 1 || b.bottom > innerHeight + 1) off.push(name);
      // The click zones run the full width at 30 px (WCAG 2.2 AA asks 24); the link pill is
      // DESIGN.md's 32 px status target. Everything else is 44.
      const zone = el.matches(".dpbtn"), pill = el.matches(".linkpill");
      if (zone ? (b.height < 24 || b.width < 44) : pill ? b.height < 32 : (b.height < 43.5 || b.width < 43.5)) small.push(name + " " + Math.round(b.width) + "×" + Math.round(b.height));
    }
    return { pic, aspect: Math.abs(pic.w / pic.h - vw / vh), scale: pic.w / innerWidth, n: els.length, bad, small, off };
  })()`);
  ok(r.aspect < 0.01, `${tag}: the picture keeps the session's aspect`, JSON.stringify(r.pic));
  ok(r.scale > 0.6 && r.scale < 0.9, `${tag}: inset to ${(r.scale * 100).toFixed(0)}% of the display`);
  ok(r.n >= 10 && !r.bad.length, `${tag}: ${r.n} controls, none over the picture`, r.bad.join(", "));
  ok(!r.off.length, `${tag}: every control on screen`, r.off.join(", "));
  ok(!r.small.length, `${tag}: every target at least 44 px`, r.small.join(", "));
  return r;
}

async function checkMapping(tag) {
  const m = await js(MAP);
  ok(m.err < 0.003, `${tag}: input maps through the transformed rect (max error ${m.err.toFixed(4)})`);
  return m;
}

async function checkToggle(tag) {
  const k0 = (await js(MAP)).k;
  await js(`(() => { const b = document.querySelector("[data-arrange='toggle']"); b.focus(); b.click(); return true; })()`);
  ok(await until("document.documentElement.dataset.arr === 'cockpit'", 3000), `${tag}: Full screen → the cockpit`);
  // Mid-morph: freeze every animation a fifth of the way through and map against the picture there.
  await sleep(40);
  await js(`document.getAnimations().forEach(a => { a.pause(); a.currentTime = 124; }); true`);
  const mid = await js(MAP);
  const midW = await js(`document.getElementById("wado-video").getBoundingClientRect().width`);
  ok(mid.err < 0.003 && midW > 0 && midW < (await js("innerWidth")) - 1, `${tag}: mid-morph, a tap still lands under the finger (max error ${mid.err.toFixed(4)}, picture ${midW.toFixed(0)} px wide)`);
  await finish();
  await sleep(100);
  const after = await js(`(() => { const b = document.querySelector("[data-arrange='toggle']"), v = document.getElementById("wado-video").getBoundingClientRect();
    return { pressed: b.getAttribute("aria-pressed"), label: b.getAttribute("aria-label"), focus: document.activeElement === b,
      full: Math.abs(v.width - innerWidth) < 1 && Math.abs(v.left) < 1, surroundInert: document.getElementById("surround").hasAttribute("inert") }; })()`);
  ok(await until(`document.querySelector("[data-arrange='toggle']").getAttribute("aria-label") === "Leave full screen"`, 2000) && after.full,
    `${tag}: cockpit — the picture fills the display, the toggle reads "Leave full screen"`, JSON.stringify(after));
  ok(after.focus && after.surroundInert, `${tag}: focus stayed on the same node; the console's own controls are inert`, JSON.stringify(after));
  const mc = await js(MAP);
  ok(mc.err < 0.003 && mc.k < k0, `${tag}: cockpit mapping exact, and a finger moves less content than in the console (${mc.k.toFixed(3)} < ${k0.toFixed(3)})`);
  return mc;
}

async function back(tag) {
  await js(`document.querySelector("[data-arrange='toggle']").click(); true`);
  ok(await until("document.documentElement.dataset.arr === 'console'", 3000), `${tag}: Leave full screen → the console`);
  await sleep(700);
  await finish();
  ok((await js(MAP)).err < 0.003, `${tag}: back in the console, mapping exact`);
}

const FORMS = [
  ["390x844", 390, 844, {}, "portrait"],
  ["844x390", 844, 390, {}, "landscape"],
  ["820x1180", 820, 1180, {}, "tablet-portrait"],
  ["1280x800", 1280, 800, { mobile: false }, "desktop"],
];
for (const [tag, w, h, o, name] of FORMS) {
  await open(w, h, o);
  await checkLayout(tag);
  await checkMapping(tag + " console");
  await shot(`${name}-console-dark`);
  await checkToggle(tag);
  // The cockpit's dock fades when idle (js/bar.js), as it always has: wake it for the picture.
  await js("window.__wado.bar && window.__wado.bar.wake(); true");
  await shot(`${name}-cockpit-dark`);
  await back(tag);
  await open(w, h, { ...o, theme: "catppuccin-latte" });
  await shot(`${name}-console-latte`);
}

// The faces, the encoder chip, reduced motion.
for (const face of ["keys", "quick", "gamepad"]) {
  await open(390, 844, { face });
  ok(await js(`document.querySelector(".cdeck").dataset.face === ${JSON.stringify(face)}`), `390×844: the ${face} face`);
  await shot(`portrait-face-${face}`);
}
await open(844, 390, { face: "keys" });
await shot("landscape-face-keys");
await open(844, 390, { face: "gamepad" });
await shot("landscape-face-gamepad");
await open(390, 844, { sw: true });
ok(await js(`/Software encoding/.test(document.querySelector(".cname").textContent)`), "software encoding: the chip takes the name's place in the band");
await shot("portrait-software-encode");

// Changing face from the switcher reaches the deck, and the deck's keys reach the session.
await open(390, 844);
await js(`window.__sent = []; window.__wado.sendInput = (m) => window.__sent.push(m);
  window.__wado.coalesce.now = (m) => window.__sent.push(m); true`);
await js(`document.querySelector(".cfaces [data-face='keys']").click(); true`);
ok(await until(`document.querySelector(".cdeck").dataset.face === "keys"`, 3000), "tapping Keys swaps the deck's face");
await js(`document.querySelector("[data-dmod='ctrl']").click(); document.querySelector("[data-dkey='tab']").click(); true`);
const sent = await js("window.__sent.map(m => m.t + ':' + m.code + ':' + m.pressed).join(' ')");
ok(sent === "key:29:true key:15:true key:15:false key:29:false", "Ctrl latched, then Tab: Ctrl+Tab to the session, and the latch clears", sent);

await open(390, 844, { reduced: true });
await js(`document.querySelector("[data-arrange='toggle']").click(); true`);
await sleep(30);
const travel = await js(`document.getAnimations().filter(a => a.effect && a.effect.getKeyframes().some(k => k.translate || k.transform)).length`);
ok(travel === 0 && (await until("document.documentElement.dataset.arr === 'cockpit'", 2000)), "reduced motion: the arrangement changes with no travel (a fade only)", "travelling: " + travel);

cleanup();
console.log(failures ? `\n${failures} failed` : `\nall passed — screenshots in ${OUT}`);
process.exit(failures ? 1 : 0);
