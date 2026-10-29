// wado bridge — the computer's actions (crates/server/src/actions): which it can run, and
// running one, over the sealed relay link. No trackpad needed: the daemon answers `actions` and
// `run` before any pad exists (`server::pad::actions`), on the trackpad's own `pad_in` /
// `pad_out` wire. The Trackpad page lists them; widgets' quick-action chips run them.
//
// Chains the `pad_out` handler `pad_link.js` registers — that file still gets every message.

W.actionsGet = () => !!(W.relaySendMsg && W.relaySendMsg({ type: "pad_in", msg: { k: "actions" } }));
W.actionRun = (id) => !!(W.relaySendMsg && W.relaySendMsg({ type: "pad_in", msg: { k: "run", id: String(id) } }));

{
  const prev = W._relayHandlers.pad_out;
  W.relayOn("pad_out", (m) => {
    if (prev) prev(m);
    const msg = m && m.msg;
    if (!msg) return;
    if (msg.k === "action_list") emit({ type: "actions", actions: msg.actions || [] });
    else if (msg.k === "ran" && !msg.ok) emit({ type: "ran", id: msg.id, message: msg.message || "That did not work." });
  });
}
