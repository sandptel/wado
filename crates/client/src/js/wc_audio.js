// wado bridge — low-latency pipeline: Opus decoded by the browser (WebCodecs) and played through an
// AudioWorklet from a buffer **we** size. This replaces Chrome's audio jitter buffer, which on a
// jittery link grew to 450–600 ms and held the picture to it (measured 2026-10-03).
//
// The buffer target follows the measured arrival jitter (p95 over 3 s) and is capped: on a spike
// bigger than the cap, late audio is dropped and the gap concealed with silence — the clock keeps
// moving, so the picture is never held for it. The worklet reports which daemon timestamp is
// being heard; that is the clock video presents on (A/V sync is mandatory).
const WC_MIN_MS = 20, WC_MAX_MS = 150;

const WORKLET = `
class WadoPlayer extends AudioWorkletProcessor {
  constructor() {
    super();
    this.q = []; this.off = 0; this.target = 40 * 48; this.now = null; this.n = 0; this.started = false;
    this.port.onmessage = (e) => {
      const m = e.data;
      if (m.target) { this.target = m.target * 48; return; }
      // Too late to play in order: drop it, never wait for it.
      if (this.now !== null && m.ts + m.l.length * 1e6 / 48000 < this.now) return;
      this.q.push(m);
      // More queued than the target allows: skip the oldest — latency stays bounded.
      while (this.q.length > 1 && this.queued() > this.target + 960) { this.q.shift(); this.off = 0; }
    };
  }
  queued() { let s = -this.off; for (const c of this.q) s += c.l.length; return s; }
  process(_, outputs) {
    const out = outputs[0], L = out[0], R = out[1] || out[0], n = L.length;
    if (!this.started) { if (this.queued() < this.target) return true; this.started = true; }
    for (let i = 0; i < n; i++) {
      const c = this.q[0];
      if (!c) { L[i] = 0; R[i] = 0; if (this.now !== null) this.now += 1e6 / 48000; continue; }
      L[i] = c.l[this.off]; R[i] = c.r[this.off];
      this.now = c.ts + this.off * 1e6 / 48000;
      if (++this.off >= c.l.length) { this.q.shift(); this.off = 0; }
    }
    if ((this.n += n) >= 480) { this.n = 0; this.port.postMessage({ ts: this.now, queued: this.queued() / 48 }); }
    return true;
  }
}
registerProcessor("wado-player", WadoPlayer);
`;

W.wcAudio = {
  ctx: null,
  node: null,
  dec: null,
  gain: null,
  heard: null,      // { ts, at }: daemon µs heard at performance.now() `at`
  targetMs: WC_MIN_MS,
  queuedMs: null,
  _off: [],
  async ensure() {
    if (this.ctx || this._starting) return;
    this._starting = true;
    try {
      const ctx = new AudioContext({ sampleRate: 48000, latencyHint: "interactive" });
      const url = URL.createObjectURL(new Blob([WORKLET], { type: "text/javascript" }));
      await ctx.audioWorklet.addModule(url);
      const node = new AudioWorkletNode(ctx, "wado-player", { outputChannelCount: [2] });
      const gain = ctx.createGain();
      node.connect(gain).connect(ctx.destination);
      node.port.onmessage = (e) => {
        if (e.data.ts !== null) this.heard = { ts: e.data.ts, at: performance.now() };
        this.queuedMs = e.data.queued;
      };
      const dec = new AudioDecoder({
        output: (ad) => this.out(ad),
        error: (e) => { if (W.rlog) W.rlog("wc: audio decoder error " + e.message); },
      });
      dec.configure({ codec: "opus", sampleRate: 48000, numberOfChannels: 2 });
      Object.assign(this, { ctx, node, gain, dec });
      this.apply();
      // Created without a gesture it starts suspended on a phone; the next tap resumes it.
      if (ctx.state !== "running") {
        const go = () => ctx.resume().catch(() => {});
        addEventListener("pointerdown", go, { once: true, capture: true });
      }
    } catch (e) {
      if (W.rlog) W.rlog("wc: audio unavailable — " + e.message);
    }
  },
  packet(body, ts) {
    if (!this.dec) { this.ensure(); return; }
    // Arrival jitter: offset = arrival − send time, up to a constant; its spread is the jitter.
    const off = performance.now() * 1000 - ts;
    this._off.push(off);
    if (this._off.length > 300) this._off.shift();
    if (this._off.length % 50 === 0) {
      const s = [...this._off].sort((a, b) => a - b);
      const jitterMs = (s[Math.floor(s.length * 0.95)] - s[0]) / 1000;
      const t = Math.round(Math.min(WC_MAX_MS, Math.max(WC_MIN_MS, jitterMs + 10)));
      if (t !== this.targetMs) { this.targetMs = t; this.node.port.postMessage({ target: t }); }
    }
    try { this.dec.decode(new EncodedAudioChunk({ type: "key", timestamp: ts, data: body })); } catch (_) {}
  },
  out(ad) {
    const n = ad.numberOfFrames;
    const l = new Float32Array(n), r = new Float32Array(n);
    try {
      ad.copyTo(l, { planeIndex: 0, format: "f32-planar" });
      ad.copyTo(r, { planeIndex: ad.numberOfChannels > 1 ? 1 : 0, format: "f32-planar" });
      this.node.port.postMessage({ ts: ad.timestamp, l, r }, [l.buffer, r.buffer]);
    } catch (_) {}
    ad.close();
  },
  // The daemon timestamp being heard right now, or null without a running audio clock.
  clockUs() {
    const h = this.heard;
    if (!h || !this.ctx || this.ctx.state !== "running") return null;
    const sinceMs = performance.now() - h.at;
    if (sinceMs > 500) return null;
    const outMs = ((this.ctx.outputLatency || 0) + (this.ctx.baseLatency || 0)) * 1000;
    return h.ts + (sinceMs - outMs) * 1000;
  },
  apply() {
    if (!this.gain) return;
    const a = W.audioLevel || { volume: 1, muted: false };
    this.gain.gain.value = a.muted ? 0 : a.volume;
  },
  unlock() { if (this.ctx) this.ctx.resume().catch(() => {}); },
};
{
  // Volume, mute and the unlock gesture reach this path too.
  const set = W.setAudio, unlock = W.audioUnlock;
  W.setAudio = (volume, muted) => { W.audioLevel = { volume, muted }; set(volume, muted); W.wcAudio.apply(); };
  W.audioUnlock = () => { unlock(); W.wcAudio.unlock(); };
}
