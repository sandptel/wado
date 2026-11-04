// wado bridge — Devices, for a device the computer lets manage them (server gate/delegate.rs):
// the trusted list with this device's codes' check words, and a code made for another device.
W.relayOn("device_list", (m) =>
  emit({ type: "device_list", devices: m.devices || [], asks: m.asks || [], error: m.error || "" }));
W.relayOn("pair_minted", (m) =>
  emit({ type: "pair_minted", link: m.link || "", typed: m.typed || "", until: m.until || 0, error: m.error || "" }));
W.devicesSend = (msg) => W.relaySendMsg(msg);
