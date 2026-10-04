// wado bridge — low-latency pipeline, the main-thread half: open the media data channel, hand
// every chunk to the worker (wc_worker.js) as a transfer, give the worker the canvas and the
// audio player's port, and keep the facades the stats line and the UI read. Nothing heavy runs
// here: decode, paint and audio all happen off this thread, which also handles touch.
const MEDIA_CHANNEL = "wado-media"; // must match wado_protocol::MEDIA_CHANNEL

// What the stats line, the latency badge and input mapping read, refreshed from the worker.
W.wcVideo = { shown: 0, late: 0, codec: "", decMs: null, lastTs: 0, q: null, _hw: "" };

W.wc = {
  dc: null,
  worker: null,
  live: false,
  lost: 0,
  size: null,
  syncMs: null,
  open(pc) {
    const dc = pc.createDataChannel(MEDIA_CHANNEL, { ordered: false, maxRetransmits: 0 });
    dc.binaryType = "arraybuffer";
    dc.onmessage = (e) => {
      if (!(e.data instanceof ArrayBuffer)) return;
      this.ensureWorker();
      this.worker.postMessage({ buf: e.data }, [e.data]);
    };
    this.dc = dc;
    if (this.worker) this.worker.postMessage({ reset: true });
  },
  ensureWorker() {
    if (this.worker) return;
    const url = URL.createObjectURL(new Blob([WC_WORKER_SRC], { type: "text/javascript" }));
    const w = new Worker(url);
    w.onmessage = (e) => {
      const m = e.data;
      if (m.kf) this.askKey();
      else if (m.log) { if (W.rlog) W.rlog(m.log); }
      else if (m.stats) this.stats(m.stats);
    };
    this.worker = w;
    this.giveCanvas();
    W.wcAudio.ensure(w);
  },
  // The canvas is transferred once; the worker paints it from then on.
  giveCanvas() {
    const c = document.getElementById("wado-canvas");
    if (!c || c._wcGiven || !this.worker) return;
    try {
      const off = c.transferControlToOffscreen();
      c._wcGiven = true;
      this.worker.postMessage({ canvas: off }, [off]);
    } catch (e) { if (W.rlog) W.rlog("wc: no OffscreenCanvas — " + e.message); }
  },
  askKey() {
    const dc = this.dc;
    if (dc && dc.readyState === "open") { try { dc.send('{"t":"kf"}'); } catch (_) {} }
  },
  stats(s) {
    if (s.live !== this.live) emit({ type: "wcLive", on: s.live });
    this.live = s.live;
    this.lost = s.lost;
    this.size = s.size;
    this.syncMs = s.syncMs;
    Object.assign(W.wcVideo, { shown: s.shown, late: s.late, codec: s.codec, decMs: s.decMs, lastTs: s.lastTs, q: s.q, _hw: s.hw });
    Object.assign(W.wcAudio, { packets: s.packets, dups: s.dups, targetMs: s.targetMs, queuedMs: s.queuedMs });
    if (s.live) this.giveCanvas(); // re-mounted stage: hand over the new element
  },
};

W.wcSupported = typeof VideoDecoder === "function" && typeof AudioDecoder === "function" &&
  typeof AudioWorkletNode === "function" && typeof Worker === "function" &&
  typeof HTMLCanvasElement !== "undefined" && "transferControlToOffscreen" in HTMLCanvasElement.prototype;
setTimeout(() => emit({ type: "caps", webcodecs: W.wcSupported, device: W.deviceName || "" }), 0);
