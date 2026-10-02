// wado bridge — this device's **seat** on the computer, as the viewer sees it.
//
// The relay keeps a seat for each device (WADO_PLAN.md, Decision Log 2026-10-02): it parks a
// device until its computer is up, holds its place while it is away, lets a human move a seat
// between devices with one tap, and lets a computer ask a connected device to approve a new
// one. `relay_link.js` carries those messages; this file is the part a person sees.
//
//   "__occupied"     every desktop is in use → offer "Use it here"
//   "__taken_over"   another device took ours → say so, offer to take it back; no auto-retry
//   approve_request  a new device wants in → Allow once / Always allow / Deny
//   approve_cleared  that request was answered elsewhere or withdrawn → drop the prompt

const seatName = (s) => s || "another device";

W.relayOn("__occupied", (msg) => {
  const who = /in use by ([^)]*)\)/.exec(msg.reason || "");
  W.sheet.ask("seat", "This desktop is in use on " + seatName(who && who[1]) + ".", [
    { label: "Use it here", primary: true, run: () => W.relayTakeover() },
    { label: "Not now" },
  ]);
});

W.relayOn("__taken_over", (msg) => {
  W.sessionOn = false;
  emit({ type: "sessionOff" });
  status("relay: this desktop is now open on " + seatName(msg.by));
  W.sheet.ask("seat", "This desktop was opened on " + seatName(msg.by) + ".", [
    { label: "Use it here", primary: true, run: () => W.relayTakeover() },
    { label: "Leave it there" },
  ]);
});

// A seat that came back means any "in use" prompt is stale.
W.relayOn("__up", ((prev) => (msg) => { W.sheet.close("seat"); if (prev) prev(msg); })(W._relayHandlers["__up"]));

W.relayOn("approve_request", (msg) => {
  const answer = (verdict) => () => W.relaySendMsg({ type: "approve_answer", id: msg.id, verdict });
  W.sheet.ask("approve:" + msg.id,
    (msg.name || "A new device") + (msg.addr ? " (" + msg.addr + ")" : "") + " wants to use this computer.", [
      { label: "Always allow", primary: true, run: answer("always") },
      { label: "Allow once", run: answer("once") },
      { label: "Deny", run: answer("deny") },
    ]);
  if (W.rlog) W.rlog("approval asked for " + (msg.name || msg.id));
});

W.relayOn("approve_cleared", (msg) => W.sheet.close("approve:" + msg.id));
