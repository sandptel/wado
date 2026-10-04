// wado bridge — low-latency pipeline (Display & stream → "Low-latency pipeline (WebCodecs)"),
// the worker half. Everything heavy runs here, off the page's main thread: reassembling chunks
// (format: `server::wcmedia::wire`), H.264 decode (VideoDecoder, hardware with a software
// fallback), painting an OffscreenCanvas on the worker's own frame loop, and Opus decode
// (AudioDecoder) fed straight to the AudioWorklet through a MessageChannel.
//
// Why a worker: v1 did all of this on the main thread — the thread that also handles touch.
// Measured on the phone (2026-10-03): 0–83 fps and touch glitching with it on, against a steady
// 90–120 fps on the standard path. The main thread now only forwards chunks (transferred, not
// copied) and receives four small stats messages a second.
//
// Sync: the worklet reports which daemon timestamp is being heard; a frame is painted once the
// sound is at its time (A/V sync is mandatory — Decision Log 2026-10-03). No audio clock: frames
// paint as they decode.
const WC_WORKER_SRC = String.raw`
"use strict";
const HEADER = 18, REORDER_MS = 30, MIN_MS = 20, MAX_MS = 150;
let canvas = null, ctx = null, feed = null, outMs = 0;
const partial = new Map(), ready = new Map();
let next = -1, needKey = true, lost = 0, kfAt = 0, lastFrameAt = -Infinity;
let dec = null, codec = "", wanted = "", hw = "", forceSw = false, configuredAt = 0;
const queue = [], sent = new Map();
let shown = 0, late = 0, decMs = null, lastTs = 0, fedAt = 0, outAt = 0, slowSince = null, size = null;
let heard = null, adec = null, dups = 0, packets = 0, targetMs = MIN_MS, queuedMs = null;
const seen = new Set(), offs = [];
const now = () => performance.now();
const log = (line) => postMessage({ log: "wc: " + line });

function askKey() { postMessage({ kf: true }); kfAt = now(); }
function maybeAskKey() { if (now() - kfAt > 250) askKey(); }
function live() { return now() - lastFrameAt < 1000; }

onmessage = (e) => {
  const m = e.data;
  if (m.buf) return chunk(m.buf);
  if (m.canvas) { canvas = m.canvas; ctx = null; return; }
  if (m.port) {
    feed = m.port;
    feed.onmessage = (ev) => {
      const d = ev.data;
      if (d.ts !== null && d.ts !== undefined) heard = { ts: d.ts, at: now() };
      if (d.queued !== undefined) queuedMs = d.queued;
    };
    return;
  }
  if (m.outMs !== undefined) { outMs = m.outMs; return; }
  if (m.reset) { reset(); return; }
};

function reset() {
  partial.clear(); ready.clear(); next = -1; needKey = true;
  try { if (dec && dec.state !== "closed") dec.close(); } catch (_) {}
  dec = null; codec = ""; wanted = "";
  for (const fr of queue) fr.close();
  queue.length = 0; sent.clear();
}

// ── chunks → frames, in order ───────────────────────────────────────────────
function chunk(buf) {
  const v = new DataView(buf);
  const kind = v.getUint8(0), key = (v.getUint8(1) & 1) === 1, seq = v.getUint32(2);
  const idx = v.getUint16(6), count = v.getUint16(8), ts = Number(v.getBigUint64(10));
  const body = new Uint8Array(buf, HEADER);
  if (kind === 1) return audio(body, ts, seq);
  lastFrameAt = now();
  let f = partial.get(seq);
  if (!f) { f = { key, ts, count, got: 0, parts: new Array(count), first: now() }; partial.set(seq, f); }
  if (!f.parts[idx]) { f.parts[idx] = body; f.got++; }
  if (f.got === f.count) { partial.delete(seq); ready.set(seq, f); drain(); }
}

function drain() {
  if (!live()) return;
  for (;;) {
    if (next < 0) {
      let k = null;
      for (const [s, f] of ready) if (f.key && (k === null || s < k)) k = s;
      if (k === null) { maybeAskKey(); break; }
      next = k;
    }
    const f = ready.get(next);
    if (f) {
      ready.delete(next); next++;
      if (f.key) needKey = false;
      if (!needKey) decode(f);
      continue;
    }
    let later = null;
    for (const s of ready.keys()) if (s > next && (later === null || s < later)) later = s;
    if (later === null || now() - ready.get(later).first < REORDER_MS) break;
    lost += later - next; next = later;
    if (!ready.get(later).key) { needKey = true; maybeAskKey(); }
  }
  const t = now();
  for (const [s, f] of partial) if (t - f.first > 1000) partial.delete(s);
  for (const s of ready.keys()) if (next >= 0 && s < next) ready.delete(s);
}
setInterval(drain, 10);

// ── video ───────────────────────────────────────────────────────────────────
function avcCodec(d) {
  for (let i = 0; i + 4 < d.length; i++) {
    if (d[i] === 0 && d[i + 1] === 0 && d[i + 2] === 1 && (d[i + 3] & 0x1f) === 7) {
      const h = (b) => b.toString(16).padStart(2, "0");
      return "avc1." + h(d[i + 4]) + h(d[i + 5]) + h(d[i + 6]);
    }
  }
  return null;
}

function decode(f) {
  let n = 0;
  for (const p of f.parts) n += p.length;
  const data = new Uint8Array(n);
  let o = 0;
  for (const p of f.parts) { data.set(p, o); o += p.length; }
  if (f.key) {
    const c = avcCodec(data);
    if (c && c !== wanted) { wanted = c; configure(c); }
  }
  if (!dec || dec.state !== "configured") return;
  sent.set(f.ts, now()); fedAt = now();
  try { dec.decode(new EncodedVideoChunk({ type: f.key ? "key" : "delta", timestamp: f.ts, data })); }
  catch (e) { log("decode threw " + e.message); needKey = true; askKey(); }
}

// The SPS's own codec string is not always accepted as written (Chrome rejects "avc1.640c2a"
// with prefer-hardware, measured), so ask before configuring: exact, constraints cleared,
// generic High, Baseline.
async function configure(c) {
  try { if (dec && dec.state !== "closed") dec.close(); } catch (_) {}
  dec = null;
  const p = c.slice(5, 7), l = c.slice(9, 11);
  const names = [...new Set([c, "avc1." + p + "00" + l, "avc1.640033", "avc1.42e01f"])];
  const prefs = forceSw ? ["prefer-software", "no-preference"] : ["prefer-hardware", "no-preference"];
  let pick = null;
  for (const h of prefs) {
    for (const name of names) {
      try {
        const r = await VideoDecoder.isConfigSupported({ codec: name, optimizeForLatency: true, hardwareAcceleration: h });
        if (r.supported) { pick = r.config; break; }
      } catch (_) {}
    }
    if (pick) break;
  }
  if (!pick) { log("no decoder accepts " + names.join(", ")); return; }
  if (wanted !== c) return;
  codec = pick.codec; hw = pick.hardwareAcceleration; decMs = null; configuredAt = now();
  dec = new VideoDecoder({ output: out, error: (e) => { log("video decoder error " + e.message); codec = ""; wanted = ""; needKey = true; askKey(); } });
  dec.configure(pick);
  needKey = true; askKey();
  log("video decoder " + pick.codec + " (stream says " + c + ", " + hw + ")");
}

function out(fr) {
  outAt = now();
  const t0 = sent.get(fr.timestamp);
  if (t0 !== undefined) {
    sent.delete(fr.timestamp);
    const d = now() - t0;
    decMs = decMs === null ? d : decMs * 0.9 + d * 0.1;
    holdBack();
  }
  queue.push(fr);
  while (queue.length > 6) { queue.shift().close(); late++; }
}

// A hardware decoder that holds frames back (measured: 300 ms to 10 s on the phone) or stops
// giving any back: rebuilt in software, once.
function toSoftware(why) {
  if (forceSw) return;
  forceSw = true;
  log("hardware decoder " + why + " — switching to software");
  const c = wanted; wanted = "";
  if (c) { wanted = c; configure(c); }
}
function holdBack() {
  if (hw === "no-preference" || hw === "prefer-software" || decMs === null) return;
  if (decMs < 66) { slowSince = null; return; }
  slowSince = slowSince || now();
  if (now() - slowSince > 2000) toSoftware("holds frames " + decMs.toFixed(0) + " ms");
}
function stall() {
  if (!dec || forceSw) return;
  if (now() - fedAt < 200 && now() - Math.max(outAt, configuredAt) > 1000) toSoftware("stalled");
}

function clockUs() {
  if (!heard) return null;
  const since = now() - heard.at;
  if (since > 500) return null;
  return heard.ts + (since - outMs) * 1000;
}

function paint(fr) {
  if (!canvas) { fr.close(); return; }
  if (canvas.width !== fr.displayWidth || canvas.height !== fr.displayHeight) {
    canvas.width = fr.displayWidth; canvas.height = fr.displayHeight; ctx = null;
  }
  ctx = ctx || canvas.getContext("2d", { alpha: false, desynchronized: true });
  ctx.drawImage(fr, 0, 0);
  size = { w: fr.displayWidth, h: fr.displayHeight };
  lastTs = fr.timestamp;
  fr.close();
  shown++;
}

function tick() {
  stall();
  const c = clockUs();
  let show = null;
  while (queue.length) {
    const fr = queue[0];
    if (c !== null && fr.timestamp > c) break;
    if (show) { show.close(); late++; }
    show = queue.shift();
    if (c === null) break;
  }
  if (show) paint(show);
}
const raf = self.requestAnimationFrame ? (f) => self.requestAnimationFrame(f) : (f) => setTimeout(f, 4);
(function loop() { try { tick(); } catch (e) { log("tick " + e.message); } raf(loop); })();

// ── audio ───────────────────────────────────────────────────────────────────
function audio(body, ts, seq) {
  // Redundant audio sends each packet up to three times: the first copy is the one used — and
  // the only one measured, since the jitter the buffer must cover is the earliest arrival's.
  if (seen.has(seq)) { dups++; return; }
  seen.add(seq);
  if (seen.size > 512) { const keep = [...seen].slice(-256); seen.clear(); for (const s of keep) seen.add(s); }
  packets++;
  if (!adec) {
    try {
      adec = new AudioDecoder({ output: pcm, error: (e) => log("audio decoder error " + e.message) });
      adec.configure({ codec: "opus", sampleRate: 48000, numberOfChannels: 2 });
    } catch (e) { log("audio decoder unavailable " + e.message); return; }
  }
  const off = now() * 1000 - ts;
  offs.push(off);
  if (offs.length > 300) offs.shift();
  if (offs.length % 50 === 0 && feed) {
    const s = [...offs].sort((a, b) => a - b);
    const t = Math.round(Math.min(MAX_MS, Math.max(MIN_MS, (s[Math.floor(s.length * 0.95)] - s[0]) / 1000 + 10)));
    if (t !== targetMs) { targetMs = t; feed.postMessage({ target: t }); }
  }
  try { adec.decode(new EncodedAudioChunk({ type: "key", timestamp: ts, data: body })); } catch (_) {}
}

function pcm(ad) {
  const n = ad.numberOfFrames, l = new Float32Array(n), r = new Float32Array(n);
  try {
    ad.copyTo(l, { planeIndex: 0, format: "f32-planar" });
    ad.copyTo(r, { planeIndex: ad.numberOfChannels > 1 ? 1 : 0, format: "f32-planar" });
    if (feed) feed.postMessage({ ts: ad.timestamp, l, r }, [l.buffer, r.buffer]);
  } catch (_) {}
  ad.close();
}

// ── to the page: 4 small messages a second ──────────────────────────────────
setInterval(() => {
  const c = clockUs();
  postMessage({ stats: {
    live: live(), shown, late, lost, codec, hw, decMs, size, lastTs, packets, dups, targetMs, queuedMs,
    syncMs: c === null || !lastTs ? null : (c - lastTs) / 1000, q: dec ? dec.decodeQueueSize : null,
  } });
}, 250);
`;
