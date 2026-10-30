// wado bridge — passkeys in the Android app (native.js; nothing without it).
//
// The app's page lives at https://appassets.androidplatform.net, an origin no Digital Asset
// Links file can vouch for, so Android's passkey providers refuse it. The app is the
// authenticator instead (android/…/passkey/): a P-256 key in the phone's Keystore per passkey,
// usable only after the person's screen lock (fingerprint, face or PIN) each time, its bytes laid
// out by the Rust core exactly as the computer verifies them. js/passkey.js is unchanged: it
// calls navigator.credentials as in any browser, and gets the app's answer.
//
//   page → app  webauthn {id, op: "create"|"get", rpId, challenge, allow: [credId…]}   (base64url)
//   app → page  webauthnResult {id, ok, rawId, clientData, attestation | authData + signature,
//                               errName, errMessage}

(() => {
  const N = W.native;
  if (!N || !navigator.credentials) return;
  const waits = new Map();
  let next = 1;
  const enc = (b) => W.b64u.enc(b instanceof ArrayBuffer ? new Uint8Array(b) : new Uint8Array(b.buffer || b));
  const buf = (s) => { const u = W.b64u.dec(s); return u.buffer.slice(u.byteOffset, u.byteOffset + u.byteLength); };

  N.on("webauthnResult", (m) => {
    const w = waits.get(m.id);
    if (!w) return;
    waits.delete(m.id);
    if (!m.ok) return w.no(new DOMException(m.errMessage || "The passkey didn't work.", m.errName || "NotAllowedError"));
    const response = { clientDataJSON: buf(m.clientData) };
    if (m.attestation) {
      response.attestationObject = buf(m.attestation);
      response.getTransports = () => ["internal"];
    } else {
      response.authenticatorData = buf(m.authData);
      response.signature = buf(m.signature);
      response.userHandle = null;
    }
    w.ok({
      type: "public-key", id: m.rawId, rawId: buf(m.rawId), response,
      authenticatorAttachment: "platform", getClientExtensionResults: () => ({}),
    });
  });

  const ask = (op, pk) => new Promise((ok, no) => {
    const id = next++;
    waits.set(id, { ok, no });
    N.send("webauthn", {
      id, op,
      rpId: (op === "create" ? pk.rp && pk.rp.id : pk.rpId) || location.hostname,
      challenge: enc(pk.challenge),
      allow: (pk.allowCredentials || []).map((c) => enc(c.id)),
    });
  });

  const c = navigator.credentials;
  const create = c.create.bind(c), get = c.get.bind(c);
  c.create = (o) => (o && o.publicKey ? ask("create", o.publicKey) : create(o));
  c.get = (o) => (o && o.publicKey ? ask("get", o.publicKey) : get(o));
  if (window.PublicKeyCredential) {
    try { PublicKeyCredential.isUserVerifyingPlatformAuthenticatorAvailable = async () => true; } catch (_) {}
  }
  // js/passkey.js asked before this file ran; its next report to Rust carries the answer.
  if (W.passkey) { W.passkey.supported = true; W.passkey.platform = true; }
})();
