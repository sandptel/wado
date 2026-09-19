// wado bridge — positional-event coalescer.
//
// One job: collapse a burst of high-rate positional events into at most one send per
// animation frame, keyed by kind, newest-wins.
//
// Why this exists: a modern mouse reports at 125–1000 Hz. Forwarding every `pointermove`
// verbatim (which `window_drag` used to do) floods the input data channel — the send
// buffer backs up and every later event queues behind the backlog, so the drag falls
// further behind the longer it lasts. Coalescing caps the rate at the display's refresh,
// which is the fastest rate the remote end can actually show anyway.
//
// Ordering is the subtle part. A pending motion must never survive past a terminal event
// for the same interaction, or the server replays a stale position *after* the release and
// the window snaps backwards. `W.coalesce.now()` exists for exactly that: it flushes the
// queue first, then sends, preserving order.

W.coalesce = {
  // kind -> latest payload awaiting this frame's flush.
  _pending: new Map(),
  _raf: null,

  _flush() {
    W.coalesce._raf = null;
    const pending = W.coalesce._pending;
    if (pending.size === 0) return;
    // Snapshot and clear BEFORE sending: a send can synchronously re-enter this module
    // (an error handler re-queueing), and we must not iterate a mutating map.
    const batch = Array.from(pending.values());
    pending.clear();
    for (const payload of batch) W.sendInput(payload);
  },

  // Queue a positional update. Only the newest payload per `kind` survives the frame.
  queue(kind, payload) {
    this._pending.set(kind, payload);
    if (this._raf == null) this._raf = requestAnimationFrame(this._flush);
  },

  // Queue an *additive* update: `dx`/`dy` sum into whatever is already pending instead of
  // replacing it. Newest-wins is right for a position and wrong for a movement — a mouse
  // reporting at 1000 Hz into a 60 Hz flush would have fifteen sixteenths of every gesture
  // thrown away, so a fast flick would travel a fraction of the distance it should.
  add(kind, payload) {
    const prev = this._pending.get(kind);
    if (prev) {
      payload.dx += prev.dx;
      payload.dy += prev.dy;
    }
    this._pending.set(kind, payload);
    if (this._raf == null) this._raf = requestAnimationFrame(this._flush);
  },

  // Send `payload` immediately, flushing anything already queued so ordering holds.
  // Use for terminal/stateful events (button, key, drag up, touch down/up).
  now(payload) {
    this._flush();
    W.sendInput(payload);
  },

  // Forget queued updates without sending (session teardown — a stale position must not
  // land on the next session).
  clear() {
    this._pending.clear();
    if (this._raf != null) {
      cancelAnimationFrame(this._raf);
      this._raf = null;
    }
  },
};
