// wado bridge — live logs (SSE). Streams the server's /events feed and emits each line to
// Rust, which renders them in the log panel. EventSource auto-reconnects on error.

W.connectLogs = (server) => {
  W.server = server;
  if (W.logES) { try { W.logES.close(); } catch (_) {} }
  const es = new EventSource(server + "/events");
  W.logES = es;
  es.onmessage = (ev) => emit({ type: "log", line: ev.data });
  es.onerror = () => {}; // EventSource auto-reconnects
};
