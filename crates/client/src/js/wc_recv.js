// wado bridge — low-latency pipeline (Display & stream → "Low-latency pipeline (WebCodecs)"):
// the media data channel, and video frames reassembled from its chunks and handed to the
// decoder in order. Format: `server::wcmedia::wire`. Audio chunks go straight to wc_audio.js.
//
// The channel is unordered and unreliable, so frames can complete out of order and some never
// do. A frame waits REORDER_MS for its predecessor; after that the predecessor counts as lost,
// and until a keyframe arrives the deltas that depend on it are skipped (3.3 adds NACK first).
const MEDIA_CHANNEL = "wado-media"; // must match wado_protocol::MEDIA_CHANNEL
const WC_HEADER = 18;
const REORDER_MS = 30;

W.wc = {
  dc: null,
  lastFrameAt: -Infinity, // not "a frame at t=0": in a page's first second that read as live
  lost: 0,
  size: null,
  // Opened with every peer connection; the daemon only sends on it while the session asks for
  // this pipeline, so an unused channel costs nothing.
  open(pc) {
    const dc = pc.createDataChannel(MEDIA_CHANNEL, { ordered: false, maxRetransmits: 0 });
    dc.binaryType = "arraybuffer";
    dc.onopen = () => { if (this.live) this.askKey(); };
    dc.onmessage = (e) => { if (e.data instanceof ArrayBuffer) this.chunk(e.data); };
    this.dc = dc;
    this.reset();
  },
  reset() {
    this.partial = new Map();
    this.ready = new Map();
    this.next = -1;
    this.needKey = true;
    W.wcVideo.reset();
  },
  get live() { return performance.now() - this.lastFrameAt < 1000; },
  askKey() {
    const dc = this.dc;
    if (dc && dc.readyState === "open") { try { dc.send('{"t":"kf"}'); } catch (_) {} }
    this._kfAt = performance.now();
  },
  maybeAskKey() { if (performance.now() - (this._kfAt || 0) > 250) this.askKey(); },
  chunk(buf) {
    const v = new DataView(buf);
    const kind = v.getUint8(0), key = (v.getUint8(1) & 1) === 1, seq = v.getUint32(2);
    const idx = v.getUint16(6), count = v.getUint16(8);
    const ts = Number(v.getBigUint64(10)); // µs, the daemon's one clock for audio and video
    const body = new Uint8Array(buf, WC_HEADER);
    if (kind === 1) { W.wcAudio.packet(body, ts, seq); return; }
    if (!this.live) emit({ type: "wcLive", on: true });
    this.lastFrameAt = performance.now();
    let f = this.partial.get(seq);
    if (!f) { f = { key, ts, count, got: 0, parts: new Array(count), first: performance.now() }; this.partial.set(seq, f); }
    if (!f.parts[idx]) { f.parts[idx] = body; f.got++; }
    if (f.got === f.count) { this.partial.delete(seq); this.ready.set(seq, f); this.drain(); }
  },
  drain() {
    // Nothing arriving means the pipeline is off: never ask the daemon for keyframes then.
    if (!this.live) return;
    for (;;) {
      if (this.next < 0) {
        // Starting (or restarting): only a keyframe can begin a decode.
        let k = null;
        for (const [s, f] of this.ready) if (f.key && (k === null || s < k)) k = s;
        if (k === null) { this.maybeAskKey(); break; }
        this.next = k;
      }
      const f = this.ready.get(this.next);
      if (f) {
        this.ready.delete(this.next);
        this.next++;
        if (f.key) this.needKey = false;
        if (!this.needKey) W.wcVideo.decode(f);
        continue;
      }
      let later = null;
      for (const s of this.ready.keys()) if (s > this.next && (later === null || s < later)) later = s;
      if (later === null || performance.now() - this.ready.get(later).first < REORDER_MS) break;
      // The frame before `later` never completed: lost. Skip to it; a delta cannot decode
      // without what it refers to, so it waits for a keyframe.
      this.lost += later - this.next;
      this.next = later;
      if (!this.ready.get(later).key) { this.needKey = true; this.maybeAskKey(); }
    }
    this.gc();
  },
  gc() {
    const now = performance.now();
    for (const [s, f] of this.partial) if (now - f.first > 1000) this.partial.delete(s);
    for (const s of this.ready.keys()) if (this.next >= 0 && s < this.next) this.ready.delete(s);
  },
};
setInterval(() => {
  if (!W.wc.dc) return;
  W.wc.drain();
  if (W.wc._wasLive && !W.wc.live) emit({ type: "wcLive", on: false });
  W.wc._wasLive = W.wc.live;
}, 10);

W.wcSupported = typeof VideoDecoder === "function" && typeof AudioDecoder === "function" &&
  typeof AudioWorkletNode === "function";
setTimeout(() => emit({ type: "caps", webcodecs: W.wcSupported, device: W.deviceName || "" }), 0);
