// wado bridge — pairing this device with a code typed from the computer's screen.
//
// A computer lets a new device in only with a code shown on it (`wado pair`, `wado tui` → Pair):
// the QR, whose link carries the code (storage.js keeps it as `wado.pair`), or the same code
// typed — eight characters, `XXXX-XXXX` — for a desktop browser or when scanning is awkward.
//
//   W.pairNormalize(text)  the code as the computer stores it, or "" if it cannot be one
//   W.pairRefused(reason)  a refusal: if it means "pair this device", stop redialling and show
//                          the form — {type:"unpaired", reason} to Rust. True if it did.
//   W.pairWith(text)       the form's submit: keep the code for the next handshake and redial
//
// The typed code is proven inside the envelope (e2e_handshake.js), never sent, and tried once.
// Then four check words show here and on the computer, and the person confirms them there.

const PAIR_TYPED = "wado.pairTyped";

// Crockford base32, as crates/server/src/gate/typed_code.rs: no I, L, O or U, and the look-alikes
// read as the digits they resemble.
W.pairNormalize = (text) => {
  const c = String(text || "").replace(/[\s-]/g, "").toUpperCase().replace(/[IL]/g, "1").replace(/O/g, "0");
  return /^[0-9A-HJKMNP-TV-Z]{8}$/.test(c) ? c : "";
};

// The computer's refusals that mean "pair this device": `UNPAIRED`, `WRONG_CODE`, `STALE_CODE`,
// `DECLINED` and `NO_ANSWER` in crates/server/src/gate/mod.rs. Matched as text because a join refusal reaches here through
// the relay, whose wire is frozen; scripts/pair-check.mjs reads that file and fails if they drift.
W.PAIR_REFUSAL = /doesn't know this device yet|code didn't work|pairing code was already used or has expired|wasn't paired/;

W._relayUnpaired = false;

W.pairRefused = (reason) => {
  if (!W.PAIR_REFUSAL.test(reason || "")) return false;
  W._relayUnpaired = true;
  // A QR code that did not work is spent either way; the form asks for a fresh one.
  try { localStorage.removeItem("wado.pair"); } catch (_) {}
  if (W.rlog) W.rlog("not paired with this computer — waiting for a code");
  emit({ type: "unpaired", reason });
  return true;
};

W.pairWith = (text) => {
  const code = W.pairNormalize(text);
  if (!code) return false;
  try { localStorage.setItem(PAIR_TYPED, code); } catch (_) { return false; }
  W._relayUnpaired = false;
  W._e2eBlocked = false;
  emit({ type: "unpaired", reason: "" });
  if (W.rlog) W.rlog("pairing with a typed code");
  return W.relayKick(false);
};
