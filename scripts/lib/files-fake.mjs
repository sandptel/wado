// A computer's files, played in the page: `W.files`' wire (connect, req, stream, mediaOpen)
// answered from memory, so the file manager and the viewer can be drawn and checked with no
// daemon behind them. Evaluated inside the page with `W` in scope: `${FILES_FAKE}`.
//
// Only for UI checks and screenshots. The real wire is covered end to end by files-e2e.mjs.
//
//   W.files.fakeEntries()        the Downloads folder: one file of every kind the viewer knows
//   W.files.fakeLane(name)       "direct" (default) or "fallback": the files lane to pretend

export const FILES_FAKE = String.raw`(() => {
  const F = W.files;
  const HOME = "/home/ana", DL = HOME + "/Downloads";
  const now = Math.floor(Date.now() / 1000);
  const enc = new TextEncoder();
  const canvasBytes = (w, h, draw, type = "image/png", q) => new Promise((ok) => {
    const c = document.createElement("canvas"); c.width = w; c.height = h;
    draw(c.getContext("2d"), w, h);
    c.toBlob((b) => b.arrayBuffer().then((a) => ok(new Uint8Array(a))), type, q);
  });
  // A photo-like picture: sky, sun, hills, a pitch.
  const landscape = (g, w, h) => {
    const sky = g.createLinearGradient(0, 0, 0, h);
    sky.addColorStop(0, "#1d3557"); sky.addColorStop(0.55, "#e76f51"); sky.addColorStop(1, "#f4a261");
    g.fillStyle = sky; g.fillRect(0, 0, w, h);
    g.fillStyle = "#ffd166"; g.beginPath(); g.arc(w * 0.7, h * 0.45, h * 0.12, 0, 7); g.fill();
    g.fillStyle = "#2a9d8f"; g.beginPath(); g.moveTo(0, h * 0.7);
    for (let x = 0; x <= w; x += w / 12) g.lineTo(x, h * (0.62 + 0.06 * Math.sin(x / w * 9)));
    g.lineTo(w, h); g.lineTo(0, h); g.fill();
    g.fillStyle = "#264653"; g.fillRect(0, h * 0.86, w, h * 0.14);
    g.strokeStyle = "#e9f5f2"; g.lineWidth = h / 200; g.strokeRect(w * 0.1, h * 0.88, w * 0.8, h * 0.2);
  };
  const portrait = (g, w, h) => {
    const bg = g.createRadialGradient(w / 2, h / 3, 10, w / 2, h / 2, h);
    bg.addColorStop(0, "#a8dadc"); bg.addColorStop(1, "#1d3557"); g.fillStyle = bg; g.fillRect(0, 0, w, h);
    g.fillStyle = "#f1faee"; g.beginPath(); g.arc(w / 2, h * 0.38, w * 0.18, 0, 7); g.fill();
    g.fillRect(w * 0.25, h * 0.58, w * 0.5, h * 0.42);
  };
  const MD = "# Trip notes\n\nThe **ferry** leaves at 07:40 from pier 3.\n\n## Packing\n\n- passport\n- charger and the *good* cable\n- [the map](https://example.org/map)\n\n> Remember: the museum is closed on Mondays.\n\n\x60\x60\x60\nbook: 2 nights, room 14\n\x60\x60\x60\n";
  const RS = "use std::collections::HashMap;\n\n/// Count the words in a line of text.\nfn count(text: &str) -> HashMap<&str, usize> {\n    let mut m = HashMap::new();\n    for w in text.split_whitespace() {\n        *m.entry(w).or_insert(0) += 1; // one more\n    }\n    m\n}\n\nfn main() {\n    let c = count(\"the cat and the hat\");\n    println!(\"{:?}\", c.get(\"the\"));\n}\n".repeat(6);
  const JSONT = JSON.stringify({ name: "wado", version: "0.9.0", private: true, scripts: { build: "dx build", test: "cargo test" }, tags: ["remote", "desktop", "wayland"], limits: { files: 1048576, ok: null } });
  const CSV = "date,city,temp_c,rain_mm\n" + Array.from({ length: 240 }, (_, i) => "2026-10-" + String(1 + (i % 30)).padStart(2, "0") + "," + ["Lisbon", "Porto", "Faro", "Braga"][i % 4] + "," + (14 + (i * 7) % 12) + "," + ((i * 13) % 9)).join("\n");
  const SVG = '<svg xmlns="http://www.w3.org/2000/svg" viewBox="0 0 200 120"><rect width="200" height="120" rx="12" fill="#264653"/><circle cx="60" cy="60" r="34" fill="#e9c46a"/><path d="M110 90 L140 30 L170 90 Z" fill="#e76f51"/><script>document.title="pwned"</script></svg>';
  const HTML = "<!doctype html><html><body><h1>Hi</h1><script>alert(1)</script></body></html>";
  const PDF = (() => {
    const objs = ["<< /Type /Catalog /Pages 2 0 R >>", "<< /Type /Pages /Kids [3 0 R] /Count 1 >>",
      "<< /Type /Page /Parent 2 0 R /MediaBox [0 0 595 842] /Contents 4 0 R /Resources << /Font << /F1 5 0 R >> >> >>",
      null, "<< /Type /Font /Subtype /Type1 /BaseFont /Helvetica >>"];
    const text = "BT /F1 28 Tf 72 760 Td (Quarterly report) Tj /F1 14 Tf 0 -40 Td (Revenue grew 12% over the summer.) Tj ET";
    objs[3] = "<< /Length " + text.length + " >>\nstream\n" + text + "\nendstream";
    let s = "%PDF-1.4\n"; const offs = [];
    objs.forEach((o, i) => { offs.push(s.length); s += (i + 1) + " 0 obj\n" + o + "\nendobj\n"; });
    const x = s.length;
    s += "xref\n0 " + (objs.length + 1) + "\n0000000000 65535 f \n" + offs.map((o) => String(o).padStart(10, "0") + " 00000 n \n").join("");
    s += "trailer\n<< /Size " + (objs.length + 1) + " /Root 1 0 R >>\nstartxref\n" + x + "\n%%EOF\n";
    return s;
  })();
  // A stored (uncompressed) zip with a few entries, central directory and all.
  const ZIP = (() => {
    const files = [["photos/", ""], ["photos/beach.jpg", "x".repeat(3000)], ["photos/hills.jpg", "y".repeat(5200)], ["notes.txt", "hello\n"], ["src/main.rs", RS]];
    const crcT = Array.from({ length: 256 }, (_, n) => { let c = n; for (let k = 0; k < 8; k++) c = c & 1 ? 0xedb88320 ^ (c >>> 1) : c >>> 1; return c >>> 0; });
    const crc = (b) => { let c = 0xffffffff; for (const x of b) c = crcT[(c ^ x) & 255] ^ (c >>> 8); return (c ^ 0xffffffff) >>> 0; };
    const parts = [], cd = []; let off = 0;
    const u16 = (v) => [v & 255, (v >> 8) & 255], u32 = (v) => [v & 255, (v >> 8) & 255, (v >> 16) & 255, (v >>> 24) & 255];
    for (const [name, body] of files) {
      const n = enc.encode(name), b = enc.encode(body), c = crc(b);
      const lh = [...u32(0x04034b50), ...u16(20), ...u16(0), ...u16(0), ...u16(0), ...u16(0x5a21), ...u32(c), ...u32(b.length), ...u32(b.length), ...u16(n.length), ...u16(0)];
      parts.push(new Uint8Array(lh), n, b);
      cd.push(new Uint8Array([...u32(0x02014b50), ...u16(20), ...u16(20), ...u16(0), ...u16(0), ...u16(0), ...u16(0x5a21), ...u32(c), ...u32(b.length), ...u32(b.length), ...u16(n.length), ...u16(0), ...u16(0), ...u16(0), ...u16(0), ...u32(name.endsWith("/") ? 0x10 : 0), ...u32(off)]), n);
      off += lh.length + n.length + b.length;
    }
    const cdLen = cd.reduce((a, b) => a + b.length, 0);
    const end = new Uint8Array([...u32(0x06054b50), ...u16(0), ...u16(0), ...u16(files.length), ...u16(files.length), ...u32(cdLen), ...u32(off), ...u16(0)]);
    const all = [...parts, ...cd, end]; const out = new Uint8Array(all.reduce((a, b) => a + b.length, 0)); let p = 0;
    for (const a of all) { out.set(a, p); p += a.length; }
    return out;
  })();
  // A second of a chord, as WAV.
  const WAV = (() => {
    const rate = 8000, n = rate * 2, b = new DataView(new ArrayBuffer(44 + n * 2));
    const str = (o, s) => [...s].forEach((ch, i) => b.setUint8(o + i, ch.charCodeAt(0)));
    str(0, "RIFF"); b.setUint32(4, 36 + n * 2, true); str(8, "WAVEfmt "); b.setUint32(16, 16, true); b.setUint16(20, 1, true);
    b.setUint16(22, 1, true); b.setUint32(24, rate, true); b.setUint32(28, rate * 2, true); b.setUint16(32, 2, true); b.setUint16(34, 16, true);
    str(36, "data"); b.setUint32(40, n * 2, true);
    for (let i = 0; i < n; i++) b.setInt16(44 + i * 2, 6000 * (Math.sin(i / rate * 2 * Math.PI * 262) + Math.sin(i / rate * 2 * Math.PI * 330)), true);
    return new Uint8Array(b.buffer);
  })();
  const BIN = Uint8Array.from({ length: 4096 }, (_, i) => (i * 37) & 255);

  // name → () => Promise<Uint8Array>, and a size the listing claims (the real one once made).
  const SRC = {
    "cristiano-ronaldo-3840x2160-14055.png": () => canvasBytes(1920, 1080, landscape),
    "IMG_2041.jpg": () => canvasBytes(900, 1200, portrait, "image/jpeg", 0.85),
    "sunset.webp": () => canvasBytes(1200, 800, landscape, "image/webp", 0.8),
    "diagram.svg": async () => enc.encode(SVG),
    "IMG_5520.heic": async () => BIN,
    "holiday.mp4": async () => BIN,
    "voice-memo.wav": async () => WAV,
    "report.pdf": async () => enc.encode(PDF),
    "trip-notes.md": async () => enc.encode(MD),
    "main.rs": async () => enc.encode(RS),
    "package.json": async () => enc.encode(JSONT),
    "weather.csv": async () => enc.encode(CSV),
    "index.html": async () => enc.encode(HTML),
    "photos.zip": async () => ZIP,
    "backup.tar.gz": async () => BIN,
    // A real font when the check found one on this machine (window.__fakeFont, base64), else noise.
    "Inter-Regular.ttf": async () => (window.__fakeFont ? Uint8Array.from(atob(window.__fakeFont), (c) => c.charCodeAt(0)) : BIN),
    "firmware.bin": async () => BIN,
    "server.log": async () => enc.encode(Array.from({ length: 9000 }, (_, i) => "2026-10-30 12:" + String(i % 60).padStart(2, "0") + " INFO request " + i + " served in " + (i % 17) + " ms").join("\n")),
  };
  const SIZES = { "cristiano-ronaldo-3840x2160-14055.png": 171008, "sunset.webp": 3565158, "holiday.mp4": 48234496, "backup.tar.gz": 734003200, "firmware.bin": 4096 };
  const made = new Map();
  const bytes = (name) => { if (!made.has(name)) made.set(name, SRC[name]()); return made.get(name); };
  const entries = () => Object.keys(SRC).map((name, i) => ({ name, dir: false, size: SIZES[name] || 2048, mtime: now - i * 3700, link: false }));
  // Real sizes, once the bytes exist.
  Promise.all(Object.keys(SRC).map((n) => bytes(n).then((b) => { if (!SIZES[n] || n === "cristiano-ronaldo-3840x2160-14055.png") SIZES[n] = b.length; }))).catch(() => {});
  const DIRS = { [HOME]: ["Desktop", "Documents", "Downloads", "Music", "Pictures", "Videos"].map((name) => ({ name, dir: true, size: 0, mtime: now - 86400, link: false })) };
  let lane = "direct";
  F.fakeLane = (l) => { lane = l; };
  F.fakeEntries = () => entries().map((e) => ({ ...e, path: DL + "/" + e.name }));
  const thumb = (name) => canvasBytes(160, 160, /portrait|IMG/.test(name) ? portrait : landscape).then((b) => btoa(String.fromCharCode(...b)));
  F.connect = () => { F.ready = true; F.lane = lane === "fallback"; F.info = { roots: [HOME], home: HOME, access: "rw", device: "Pixel 8", space: [{ root: HOME, free: 182e9, total: 512e9 }], trash: HOME + "/.local/share/Trash/files" }; return Promise.resolve(F.info); };
  F.open = () => true;
  F.forget = (id) => { gone.add(id); };
  const gone = new Set();
  F.req = async (op, a = {}) => {
    await new Promise((r) => setTimeout(r, 20));
    if (op === "hello") return F.connect();
    if (op === "quick") return { pins: [{ path: DL, name: "Downloads", kind: "folder" }, { path: HOME + "/Documents", name: "Documents", kind: "folder" }, { path: HOME + "/code", name: "code", kind: "pin" }],
      recent: F.fakeEntries().slice(0, 8).map(({ path, name, mtime, size }) => ({ path, name, mtime, size })) };
    if (op === "list") return { entries: a.path === DL ? entries() : DIRS[a.path] || [] };
    if (op === "find") return { entries: F.fakeEntries().filter((e) => !a.kind || F.kind(e.name).image === (a.kind === "image")), partial: false };
    if (op === "thumb") { const n = a.path.split("/").pop(); if (!/\.(png|jpe?g|webp|mp4|heic)$/.test(n)) throw new Error("no thumbnail"); return { png: await thumb(n) }; }
    if (op === "probe") return { duration: 2, video: null, audio: [{ codec: "pcm", lang: "" }], subs: [], sidecars: [] };
    if (op === "devices") return { devices: [] };
    return {};
  };
  let nid = 1000;
  F.stream = (op, a, h) => {
    const id = ++nid;
    const name = a.path.split("/").pop();
    (async () => {
      const b = await bytes(name);
      const off = a.offset || 0;
      h.msg({ size: b.length, mtime: now });
      for (let p = off; p < b.length; p += 61440) {
        if (gone.has(id)) return;
        h.bytes(b.subarray(p, Math.min(b.length, p + 61440)));
        await new Promise((r) => setTimeout(r, 0));
      }
      if (gone.has(id)) return;
      const sha = [...new Uint8Array(await crypto.subtle.digest("SHA-256", b))].map((x) => x.toString(16).padStart(2, "0")).join("");
      h.msg({ done: true, sha256: sha });
    })();
    return id;
  };
  // Video and audio: the daemon's MSE stream stood in for by the bytes themselves.
  F.mediaOpen = async (path, el, opts = {}) => {
    const name = path.split("/").pop();
    let url;
    if (/\.wav$/.test(name)) url = URL.createObjectURL(new Blob([await bytes(name)], { type: "audio/wav" }));
    else {
      // Two seconds of a drawn clip, recorded from a canvas.
      const c = document.createElement("canvas"); c.width = 640; c.height = 360;
      const g = c.getContext("2d"); landscape(g, 640, 360);
      const rec = new MediaRecorder(c.captureStream(15), { mimeType: "video/webm" });
      const parts = []; rec.ondataavailable = (e) => parts.push(e.data);
      const done = new Promise((r) => (rec.onstop = r));
      rec.start(); let t = 0;
      const iv = setInterval(() => { landscape(g, 640, 360); g.fillStyle = "#fff"; g.font = "28px sans-serif"; g.fillText("Holiday " + (t++), 24, 48); }, 66);
      await new Promise((r) => setTimeout(r, 600)); rec.stop(); clearInterval(iv); await done;
      url = URL.createObjectURL(new Blob(parts, { type: "video/webm" }));
    }
    el.src = url;
    if (opts.onInfo) opts.onInfo({ transcode: false });
    return { info: { duration: 2, video: /\.wav$/.test(name) ? null : { codec: "vp8" }, audio: [], subs: [], sidecars: [] }, setAudio() {}, destroy() { URL.revokeObjectURL(url); } };
  };
})()`;
