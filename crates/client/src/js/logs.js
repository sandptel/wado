// wado bridge — live logs + session events (SSE). Streams /events and emits each message
// to Rust. Unnamed data events → log lines; named events → typed messages.

W.connectLogs = (server) => {
  W.server = server;
  if (W.logES) { try { W.logES.close(); } catch (_) {} }
  const es = new EventSource(server + "/events");
  W.logES = es;
  es.onmessage = (ev) => emit({ type: "log", line: ev.data });
  // Named SSE event: the server pushes this when the encoder tier changes at runtime.
  es.addEventListener("encoder", (ev) => emit({ type: "encoder", ...JSON.parse(ev.data) }));
  // Named SSE event: the server pushes this when the session ends abnormally (render
  // panic or all encoder tiers exhausted). The client shows a reconnect banner.
  es.addEventListener("session", (ev) => emit({ type: "session", ...JSON.parse(ev.data) }));
  es.onerror = () => {}; // EventSource auto-reconnects
};
