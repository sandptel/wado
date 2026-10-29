// wado bridge — this device's **seat** on the computer, as the viewer sees it.
//
// The relay keeps a seat for each device (WADO_PLAN.md, Decision Log 2026-10-02): it parks a
// device until its computer is up, holds its place while it is away, and lets a human move a
// seat between devices with one tap. `relay_link.js` carries those messages; this file is the
// part a person sees. (No device approves another any more: a new one pairs with a code shown
// on the computer — `pair.js`.)
//
//   "__occupied"     every desktop is in use → offer "Use it here"
//   "__taken_over"   another device took ours → say so, offer to take it back; no auto-retry

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

