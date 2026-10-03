// wado bridge — low-latency pipeline: H.264 decoded by the browser (WebCodecs, hardware when the
// phone has it, `optimizeForLatency`) and painted to a canvas over the stage, each frame at the
// moment its sound is heard: the audio clock (wc_audio.js) is the master, because A/V sync is
// mandatory (Decision Log 2026-10-03). With no audio clock, frames show as soon as they decode.
W.wcVideo = {
  dec: null,
  codec: "",
  queue: [],
  decMs: null,
  shown: 0,
  late: 0,
  _sent: new Map(),
  reset() {
    try { if (this.dec && this.dec.state !== "closed") this.dec.close(); } catch (_) {}
    this.dec = null;
    this.codec = "";
    for (const fr of this.queue) fr.close();
    this.queue = [];
    this._sent.clear();
  },
  decode(f) {
    let n = 0;
    for (const p of f.parts) n += p.length;
    const data = new Uint8Array(n);
    let o = 0;
    for (const p of f.parts) { data.set(p, o); o += p.length; }
    if (f.key) {
      const c = avcCodec(data);
      if (c && c !== this._wanted) { this._wanted = c; this.configure(c); }
    }
    if (!this.dec || this.dec.state !== "configured") return;
    this._sent.set(f.ts, performance.now());
    this._fedAt = performance.now();
    try {
      this.dec.decode(new EncodedVideoChunk({ type: f.key ? "key" : "delta", timestamp: f.ts, data }));
    } catch (e) {
      if (W.rlog) W.rlog("wc: decode threw " + e.message);
      W.wc.needKey = true;
      W.wc.askKey();
    }
  },
  // The SPS's own codec string is not always one the browser accepts as written — measured:
  // `avc1.640c2a` (High 4.2 with constraint flags 0x0c) "Unsupported configuration" in Chrome,
  // which decodes that very stream once asked as `avc1.64002a`. So ask before configuring:
  // exact, constraints cleared, generic High, Baseline — hardware first, then anything.
  async configure(codec) {
    this.reset();
    const p = codec.slice(5, 7), l = codec.slice(9, 11);
    const names = [...new Set([codec, `avc1.${p}00${l}`, "avc1.640033", "avc1.42e01f"])];
    let pick = null;
    for (const hw of this._forceSoftware ? ["prefer-software", "no-preference"] : ["prefer-hardware", "no-preference"]) {
      for (const c of names) {
        try {
          const r = await VideoDecoder.isConfigSupported({ codec: c, optimizeForLatency: true, hardwareAcceleration: hw });
          if (r.supported) { pick = r.config; break; }
        } catch (_) {}
      }
      if (pick) break;
    }
    if (!pick) { if (W.rlog) W.rlog("wc: no decoder accepts " + names.join(", ")); return; }
    if (this._wanted !== codec) return; // a newer keyframe asked for something else meanwhile
    this.codec = pick.codec;
    this._hw = pick.hardwareAcceleration;
    this._configuredAt = performance.now();
    this.decMs = null;
    this.dec = new VideoDecoder({
      output: (fr) => this.out(fr),
      error: (e) => {
        if (W.rlog) W.rlog("wc: video decoder error " + e.message);
        this.codec = "";
        this._wanted = "";
        W.wc.needKey = true;
        W.wc.askKey();
      },
    });
    this.dec.configure(pick);
    // Asked for afresh: the keyframe that triggered this was decoded before the decoder existed.
    W.wc.needKey = true;
    W.wc.askKey();
    if (W.rlog) W.rlog(`wc: video decoder ${pick.codec} (stream says ${codec}, ${pick.hardwareAcceleration})`);
  },
  out(fr) {
    this._outAt = performance.now();
    const t0 = this._sent.get(fr.timestamp);
    if (t0 !== undefined) {
      this._sent.delete(fr.timestamp);
      const d = performance.now() - t0;
      this.decMs = this.decMs === null ? d : this.decMs * 0.9 + d * 0.1;
      this.checkHoldBack();
    }
    this.queue.push(fr);
    // Never a backlog: past a handful, the oldest are late by definition.
    while (this.queue.length > 6) { this.queue.shift().close(); this.late++; }
  },
  // A hardware decoder that holds frames back. Measured on the phone (2026-10-03): 300–600 ms
  // from decode() to output — MediaCodec buffering for reordering the stream never does.
  // Software decoding releases each frame as it is decoded, so past ~4 frame times for 2 s the
  // decoder is rebuilt in software, once, and the swap is logged.
  checkHoldBack() {
    if (this._hw === "no-preference" || this.decMs === null) return;
    const limit = 4 * 1000 / Math.max(30, this._fps || 60);
    const now = performance.now();
    if (this.decMs < limit) { this._slowSince = null; return; }
    this._slowSince = this._slowSince || now;
    if (now - this._slowSince < 2000) return;
    if (W.rlog) W.rlog(`wc: hardware decoder holds frames ${this.decMs.toFixed(0)} ms — switching to software`);
    this._forceSoftware = true;
    this._slowSince = null;
    const c = this._wanted;
    this._wanted = "";
    if (c) { this._wanted = c; this.configure(c); }
  },

  // A decoder that takes frames and gives nothing back for a second has stalled (measured on the
  // phone: no output at all, decode time climbing to 10 s). The hold-back check above only runs
  // when a frame comes out, so it cannot see this — rebuild in software from here.
  checkStall() {
    if (!this.dec || this._hw === "no-preference" || this._forceSoftware) return;
    const fed = this._fedAt || 0, out = this._outAt || 0, now = performance.now();
    if (now - fed < 200 && now - Math.max(out, this._configuredAt || 0) > 1000) {
      if (W.rlog) W.rlog("wc: hardware decoder stalled (no output for 1 s) — switching to software");
      this._forceSoftware = true;
      const c = this._wanted;
      if (c) { this._wanted = c; this.configure(c); }
    }
  },

  // Every display refresh: the newest frame whose time has come.
  tick() {
    this.checkStall();
    const now = W.wcAudio.clockUs();
    let show = null;
    while (this.queue.length) {
      const fr = this.queue[0];
      if (now !== null && fr.timestamp > now) break;
      if (show) { show.close(); this.late++; }
      show = this.queue.shift();
      if (now === null) break;
    }
    if (show) this.paint(show);
  },
  paint(fr) {
    const c = document.getElementById("wado-canvas");
    if (!c) { fr.close(); return; }
    if (c.width !== fr.displayWidth || c.height !== fr.displayHeight) {
      c.width = fr.displayWidth;
      c.height = fr.displayHeight;
      this._ctx = null;
    }
    this._ctx = this._ctx || c.getContext("2d", { alpha: false, desynchronized: true });
    this._ctx.drawImage(fr, 0, 0);
    W.wc.size = { w: fr.displayWidth, h: fr.displayHeight };
    this.lastTs = fr.timestamp;
    fr.close();
    this.shown++;
  },
};
// Started on the next frame, never synchronously: this file loads before wc_audio.js, and a
// throw at load time stops every bridge file after it (2026-10-03: it took `W.start` with it).
requestAnimationFrame(function loop() {
  try { if (W.wc && W.wc.live && W.wcAudio) W.wcVideo.tick(); } catch (e) { if (W.rlog) W.rlog("wc: tick " + e.message); }
  requestAnimationFrame(loop);
});

// "avc1.PPCCLL" from the stream's own SPS (NAL type 7): profile, constraints, level.
function avcCodec(d) {
  for (let i = 0; i + 4 < d.length; i++) {
    if (d[i] === 0 && d[i + 1] === 0 && d[i + 2] === 1 && (d[i + 3] & 0x1f) === 7) {
      const h = (b) => b.toString(16).padStart(2, "0");
      return "avc1." + h(d[i + 4]) + h(d[i + 5]) + h(d[i + 6]);
    }
  }
  return null;
}
