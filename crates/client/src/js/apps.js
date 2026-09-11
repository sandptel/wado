// wado bridge — the launchable-application list.
//
// Asks whichever transport is in use and emits the result to Rust, which owns the picker.
// Needs no running session: you choose what to launch before there is anything to launch it
// into, which is also why this cannot ride the WebRTC data channel.
//
// Relay mode is asynchronous by nature — the reply arrives later on the WebSocket and is
// dispatched from relay.js — so this function does not resolve with the list. Both paths end
// the same way, at `emit({type:"apps"})`.

W.requestApps = async () => {
  if (W.relayMode) {
    if (W.relayWs && W.relayWs.readyState === WebSocket.OPEN) {
      W.relayWs.send(JSON.stringify({ type: "apps_request" }));
    }
    return;
  }
  try {
    const res = await fetch(W.server + "/apps", { cache: "no-store" });
    if (res.ok) emit({ type: "apps", apps: await res.json() });
  } catch (_) {
    // An unreachable server is already visible in the status line; the picker simply stays
    // empty and the free-text box still works.
  }
};
