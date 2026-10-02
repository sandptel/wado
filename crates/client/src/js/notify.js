// wado bridge — notifications from the session's apps (server::notify), handed to Rust, which
// shows them as toasts and keeps them in the control centre's shade.
W.relayOn("notification", (msg) =>
  emit({ type: "notification", id: msg.id || 0, app: msg.app || "", summary: msg.summary || "", body: msg.body || "" }));
W.relayOn("notification_closed", (msg) => emit({ type: "notificationClosed", id: msg.id || 0 }));
