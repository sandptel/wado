// wado bridge — the launchable-application list.
//
// Asks whichever transport is in use and emits the result to Rust, which owns the picker.
// Needs no running session: you choose what to launch before there is anything to launch it
// into, which is also why this cannot ride the WebRTC data channel.
//
// Relay mode is asynchronous by nature — the reply arrives later on the WebSocket and is
// dispatched from relay.js — so this function does not resolve with the list. Both paths end
// the same way, at `emit({type:"apps"})`.

// `server` is optional and only used in direct mode. It is a parameter rather than a read of
// W.server because W.server is set as a *side effect* of connectLogs — so calling this before
// that ran would fetch a relative "/apps" against the dev server's own origin, get nothing,
// and leave an empty list indistinguishable from "this machine has no apps".
W.requestApps = async (server) => {
  if (W.relayMode) {
    if (W.relayWs && W.relayWs.readyState === WebSocket.OPEN) {
      W.relayWs.send(JSON.stringify({ type: "apps_request" }));
    }
    return;
  }
  try {
    const res = await fetch((server || W.server) + "/apps", { cache: "no-store" });
    if (res.ok) emit({ type: "apps", apps: await res.json() });
  } catch (_) {
    // An unreachable server is already visible in the status line; the picker simply stays
    // empty and the free-text box still works.
  }
};
