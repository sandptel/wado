// wado bridge — low-latency pipeline, the sound: an AudioWorklet playing from a buffer **we**
// size, fed by the worker's AudioDecoder through a MessageChannel, so audio never passes through
// the main thread. This replaces Chrome's audio jitter buffer, which on a jittery link grew to
// 450–600 ms and held the picture to it (measured 2026-10-03).
//
// The buffer target (from the worker) follows the measured arrival jitter and is capped; late
// audio is dropped and the gap concealed, so the clock keeps moving. The worklet tells the
// worker which daemon timestamp is being heard: that is the clock the picture is painted on.
const WORKLET = String.raw`
class WadoPlayer extends AudioWorkletProcessor {
  constructor() {
    super();
    this.q = []; this.off = 0; this.target = 40 * 48; this.now = null; this.n = 0; this.started = false; this.feed = null;
    this.port.onmessage = (e) => {
      if (e.data.port) { this.feed = e.data.port; this.feed.onmessage = (ev) => this.take(ev.data); }
    };
  }
  take(m) {
    if (m.target) { this.target = m.target * 48; return; }
    // Too late to play in order: drop it while there is newer audio to play; with the queue
    // empty, re-sync to it instead (dropping every arrival would keep it silent for good).
    if (this.now !== null && m.ts + m.l.length * 1e6 / 48000 < this.now) {
      if (this.q.length) return;
      this.now = null; this.started = false; this.off = 0;
    }
    this.q.push(m);
    while (this.q.length > 1 && this.queued() > this.target + 960) { this.q.shift(); this.off = 0; }
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
    if ((this.n += n) >= 480 && this.feed) { this.n = 0; this.feed.postMessage({ ts: this.now, queued: this.queued() / 48 }); }
    return true;
  }
}
registerProcessor("wado-player", WadoPlayer);
`;

W.wcAudio = {
  ctx: null,
  gain: null,
  packets: 0,
  dups: 0,
  targetMs: null,
  queuedMs: null,
  async ensure(worker) {
    if (this.ctx || this._starting) return;
    this._starting = true;
    try {
      const ctx = new AudioContext({ sampleRate: 48000, latencyHint: "interactive" });
      const url = URL.createObjectURL(new Blob([WORKLET], { type: "text/javascript" }));
      await ctx.audioWorklet.addModule(url);
      const node = new AudioWorkletNode(ctx, "wado-player", { outputChannelCount: [2] });
      const gain = ctx.createGain();
      node.connect(gain).connect(ctx.destination);
      // Worker ↔ worklet directly: decoded audio and the "now heard" clock skip this thread.
      const ch = new MessageChannel();
      node.port.postMessage({ port: ch.port1 }, [ch.port1]);
      worker.postMessage({ port: ch.port2 }, [ch.port2]);
      Object.assign(this, { ctx, gain });
      this.apply();
      // The worker needs the output latency to know when a sample is *heard*.
      setInterval(() => worker.postMessage({ outMs: ((ctx.outputLatency || 0) + (ctx.baseLatency || 0)) * 1000 }), 1000);
      if (ctx.state !== "running") addEventListener("pointerdown", () => ctx.resume().catch(() => {}), { once: true, capture: true });
    } catch (e) {
      if (W.rlog) W.rlog("wc: audio unavailable — " + e.message);
    }
  },
  apply() {
    if (!this.gain) return;
    const a = W.audioLevel || { volume: 1, muted: false };
    this.gain.gain.value = a.muted ? 0 : a.volume;
  },
  unlock() { if (this.ctx) this.ctx.resume().catch(() => {}); },
};
{
  const set = W.setAudio, unlock = W.audioUnlock;
  W.setAudio = (volume, muted) => { W.audioLevel = { volume, muted }; set(volume, muted); W.wcAudio.apply(); };
  W.audioUnlock = () => { unlock(); W.wcAudio.unlock(); };
}
