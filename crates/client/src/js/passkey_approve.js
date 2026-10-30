// wado bridge — "Approve on the computer instead": unlock without the passkey, if someone at the
// computer says yes (`wado passkey approve`, `wado tui`; daemon: crates/server/src/passkey/approve.rs).
// For the phone whose fingerprint prompt hangs, whose screen lock changed, whose passkey was made
// in another browser or app. A yes opens the same window a passkey would.
//
//   W.passkeyApprove()            ask the computer (the sheet's button), for the door on the sheet
//   W.passkeyApproveFor(what, fn) the same from a door already open (the trackpad's refusal card)
//
// `P.approve` = {state, left_s, message} — waiting | approved | declined | expired | refused |
// cancelled — goes to Rust with the rest of the passkey state. Not now withdraws the request.

(() => {
  const P = W.passkey;
  if (!P) return;
  const say = () => P._say();
  // The computer gives up at 2 minutes; this page waits that long and a little more, then says
  // so itself — a request whose answer was lost is not left waiting.
  const LIMIT = 135000;
  let limit = 0;

  W.passkeyApprove = () => {
    if (P._stop) P._stop("replaced"); // a prompt still up is not needed any more
    if (P._settle) P._settle();
    P.busy = false;
    P.note = "";
    P.kind = "";
    if (!(W.relaySendMsg && W.relaySendMsg({ type: "passkey_approve" }))) {
      P.approve = null;
      P.kind = "retry";
      P.note = "Not connected to the computer right now — try again when it is back.";
      say();
      return;
    }
    P.approve = { state: "waiting", left_s: 120, message: "" };
    clearTimeout(limit);
    limit = setTimeout(() => {
      if (!P.approve || P.approve.state !== "waiting") return;
      P.approve = { state: "expired", left_s: 0, message: "Nobody answered on the computer — try again when someone is there." };
      say();
    }, LIMIT);
    say();
  };

  W.passkeyApproveFor = (what, fn) => {
    // Through the sheet, so the person sees the request waiting and can withdraw it.
    W.passkeyThen(what, fn);
    W.passkeyApprove();
  };

  W.relayOn("passkey_approve_state", (m) => {
    if (!P.approve && m.state !== "approved") return; // withdrawn here already
    const state = String(m.state || "");
    if (state !== "waiting") clearTimeout(limit);
    // Approved: the state that follows unlocks, and runs what was waiting (js/passkey.js).
    P.approve = state === "cancelled" ? null : { state, left_s: m.left_s | 0, message: String(m.message || "") };
    if (W.rlog && state !== "waiting") W.rlog("passkey: approval on the computer " + state);
    say();
  });
})();
