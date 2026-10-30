// The client's own bridge bundle in a headless Chrome page, for e2e scripts that need the real
// js/ (reconnect-time.mjs, lane-e2e.mjs) without a dx build of the whole app.
//
// Every js/ file bridge.rs concatenates becomes its own classic script, in the same order (they
// share one global lexical scope, as inside the one eval), so a file that throws at load is
// reported by name instead of silently taking every later file with it. `emit` lands in
// `window.__ev` as [ms, msg]; load errors in `window.__errs`.
//
//   const page = await bundlePage({ dir, cdpPort, httpPort, start })   start(bin, args) spawns
//   await page.ev("window.__wado.relayDial(…)")                         evaluate, await, return
//   page.close()
import { readFileSync } from "node:fs";
import { createServer } from "node:http";
import { join } from "node:path";

const ROOT = new URL("../..", import.meta.url).pathname;
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

export function bundle() {
  const src = readFileSync(join(ROOT, "crates/client/src/bridge.rs"), "utf8");
  const at = src.indexOf("pub const JS: &str = concat!(");
  const body = src.slice(at, src.indexOf(");\n", at));
  const unesc = (s) => s.replace(/\\(.)/g, (_, c) => (c === "n" ? "\n" : c));
  const parts = [];
  let inline = "";
  for (const m of body.matchAll(/include_str!\("([^"]+)"\)|"((?:[^"\\]|\\.)*)"/g)) {
    if (m[1] && m[1].startsWith("js/") && m[1].endsWith(".js")) {
      if (inline.trim()) parts.push({ name: "inline", code: inline });
      inline = "";
      let code = readFileSync(join(ROOT, "crates/client/src", m[1]), "utf8");
      // The eval is an async function body; a classic script cannot await at top level.
      if (m[1] === "js/lifecycle.js") code = code.replace(/\nawait new Promise\(\(\) => \{\}\);\s*$/, "\n");
      parts.push({ name: m[1], code });
    } else if (m[1]) inline += readFileSync(join(ROOT, "crates/client/src", m[1]), "utf8");
    else inline += unesc(m[2]);
  }
  if (inline.trim()) parts.push({ name: "inline", code: inline });
  return parts;
}

export async function bundlePage({ dir, cdpPort, httpPort, start }) {
  const parts = bundle();
  const page = `<!doctype html><html><body>
<video id="wado-video" muted autoplay playsinline></video>
<script>
  window.__ev = []; window.__errs = [];
  window.dioxus = { send: (m) => window.__ev.push([performance.now(), m]) };
  window.addEventListener("error", (e) => window.__errs.push((e.filename || "") + ": " + e.message));
</script>
${parts.map((_, i) => `<script src="/b/${i}.js"></script>`).join("\n")}
</body></html>`;
  const http = createServer((req, res) => {
    const m = /^\/b\/(\d+)\.js$/.exec(req.url);
    if (m) { res.writeHead(200, { "content-type": "text/javascript" }); return res.end(parts[+m[1]].code); }
    res.writeHead(200, { "content-type": "text/html" });
    res.end(page);
  });
  await new Promise((r) => http.listen(httpPort, "127.0.0.1", r));
  start("google-chrome-stable", ["--headless=new", "--remote-debugging-port=" + cdpPort, "--user-data-dir=" + join(dir, "chrome-" + cdpPort),
    "--autoplay-policy=no-user-gesture-required", "--no-first-run", "about:blank"]);
  let list;
  for (let i = 0; i < 60 && !list; i++) { await sleep(200); try { list = await (await fetch(`http://127.0.0.1:${cdpPort}/json/list`)).json(); } catch {} }
  const ws = new WebSocket(list.find((t) => t.type === "page").webSocketDebuggerUrl);
  await new Promise((r) => (ws.onopen = r));
  let id = 0;
  const pend = new Map();
  ws.onmessage = (e) => { const m = JSON.parse(e.data); if (m.id && pend.has(m.id)) { pend.get(m.id)(m); pend.delete(m.id); } };
  const cdp = (method, params = {}) => new Promise((r) => { const i = ++id; pend.set(i, r); ws.send(JSON.stringify({ id: i, method, params })); });
  const ev = async (expr) => {
    const r = await cdp("Runtime.evaluate", { expression: expr, returnByValue: true, awaitPromise: true });
    if (r.result?.exceptionDetails) throw new Error(JSON.stringify(r.result.exceptionDetails).slice(0, 400));
    return r.result?.result?.value;
  };
  await cdp("Page.navigate", { url: `http://127.0.0.1:${httpPort}/` });
  await sleep(1500);
  return { cdp, ev, errors: () => ev("window.__errs"), close: () => { try { ws.close(); } catch {} http.close(); } };
}

// Pair this page with a computer by the QR's code, as storage.js does for the connect link.
export async function pairPage(page, rid, qr) {
  await page.ev(`(() => {
    localStorage.setItem("wado.pair", ${JSON.stringify(qr.pair)});
    localStorage.setItem("wado.hostpins", JSON.stringify({ ${JSON.stringify(rid)}: { pin: ${JSON.stringify(qr.hk)}, from: "qr" } }));
    return true;
  })()`);
}
