// wado bridge — session control: launch, and the window actions.
//
// One sender for both transports and every verb. Direct mode POSTs a `SessionControl` to
// /session/control; relay mode sends the matching RelayMsg over the existing WebSocket, where
// one variant per action is the idiom (see the protocol's control.rs for why the two sides
// differ on purpose).
//
// Window actions deliberately do NOT ride the WebRTC data channel, even though it is open and
// faster. That channel carries input, and invariant #1 keeps it free of anything else.

// `body` is a serde-shaped SessionControl: {Launch:{command}} or {Window:"maximize"}.
const sendControl = async (body, relayMsg) => {
  if (!W.sessionOn) { status("ignored — no session"); return; }
  if (W.relayMode) {
    if (!W.relayWs || W.relayWs.readyState !== WebSocket.OPEN) {
      status("relay: not connected");
      return;
    }
    W.relayWs.send(JSON.stringify(relayMsg));
    return;
  }
  try {
    const res = await fetch(W.server + "/session/control", {
      method: "POST",
      headers: { "Content-Type": "application/json" },
      body: JSON.stringify(body),
    });
    if (!res.ok) status("control failed: " + (await res.text()));
  } catch (e) {
    status("control error: " + (e && e.message ? e.message : e));
  }
};

// Launch a command into the running session (callable repeatedly).
W.launch = (command) =>
  sendControl({ Launch: { command } }, { type: "session_launch", command });

// One of "maximize" | "minimize" | "close" | "cycle_focus", acting on the focused window.
W.windowAction = (action) =>
  sendControl({ Window: action }, { type: "session_window", action });
