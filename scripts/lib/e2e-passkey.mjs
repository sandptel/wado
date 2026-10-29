// A software passkey for the e2e scripts and for the daemon's WebAuthn test vectors.
//
// It builds exactly what a browser hands the page after `navigator.credentials.create()` /
// `.get()` with `attestation: "none"` and ES256 — clientDataJSON, a CBOR attestation object with
// the COSE key, authenticator data, a DER signature — using WebCrypto, so the daemon's checks
// (`crates/server/src/passkey/`) are exercised against bytes made by an independent
// implementation rather than by itself.
//
//   const pk = await authenticator();                    one credential, counter from 0
//   const reg = await pk.register(challengeB64u, origin) {client_data, attestation} (base64url)
//   const as = await pk.assert(challengeB64u, origin)    {cred_id, client_data, auth_data, signature}
//   pk.counter = 0                                        a synced passkey that never counts

const subtle = globalThis.crypto.subtle;
export const b64u = (b) => Buffer.from(b).toString("base64url");
export const unb64u = (s) => new Uint8Array(Buffer.from(s, "base64url"));
const sha256 = async (b) => new Uint8Array(await subtle.digest("SHA-256", b));
const cat = (...a) => {
  const out = new Uint8Array(a.reduce((n, x) => n + x.length, 0));
  let i = 0;
  for (const x of a) { out.set(x, i); i += x.length; }
  return out;
};
const u32 = (n) => new Uint8Array([n >>> 24, (n >>> 16) & 255, (n >>> 8) & 255, n & 255]);

// ── Just enough CBOR to write an attestation object and a COSE key ────────────
function head(major, n) {
  if (n < 24) return new Uint8Array([(major << 5) | n]);
  if (n < 256) return new Uint8Array([(major << 5) | 24, n]);
  return new Uint8Array([(major << 5) | 25, n >> 8, n & 255]);
}
function cbor(v) {
  if (typeof v === "number") return v >= 0 ? head(0, v) : head(1, -1 - v);
  if (typeof v === "string") { const b = new TextEncoder().encode(v); return cat(head(3, b.length), b); }
  if (v instanceof Uint8Array) return cat(head(2, v.length), v);
  if (v instanceof Map) return cat(head(5, v.size), ...[...v].flatMap(([k, x]) => [cbor(k), cbor(x)]));
  throw new Error("cbor: unsupported " + typeof v);
}

// WebCrypto signs ECDSA as r‖s; WebAuthn carries it as an ASN.1 DER SEQUENCE of two INTEGERs.
function der(raw) {
  const int = (b) => {
    let i = 0;
    while (i < b.length - 1 && b[i] === 0) i++;
    b = b.slice(i);
    if (b[0] & 0x80) b = cat(new Uint8Array([0]), b);
    return cat(new Uint8Array([2, b.length]), b);
  };
  const body = cat(int(raw.slice(0, 32)), int(raw.slice(32)));
  return cat(new Uint8Array([0x30, body.length]), body);
}

const FLAG_UP = 0x01, FLAG_UV = 0x04, FLAG_AT = 0x40;

export async function authenticator({ uv = true } = {}) {
  const keys = await subtle.generateKey({ name: "ECDSA", namedCurve: "P-256" }, true, ["sign", "verify"]);
  const raw = new Uint8Array(await subtle.exportKey("raw", keys.publicKey)); // 0x04‖x‖y
  const credId = globalThis.crypto.getRandomValues(new Uint8Array(16));
  const flags = FLAG_UP | (uv ? FLAG_UV : 0);
  const me = {
    credId: b64u(credId),
    counter: 0,
    keys,
    clientData(type, challenge, origin) {
      return new TextEncoder().encode(JSON.stringify({ type, challenge, origin, crossOrigin: false }));
    },
    async register(challenge, origin) {
      const rpId = new URL(origin).hostname;
      const cose = cbor(new Map([[1, 2], [3, -7], [-1, 1], [-2, raw.slice(1, 33)], [-3, raw.slice(33)]]));
      const authData = cat(
        await sha256(new TextEncoder().encode(rpId)),
        new Uint8Array([flags | FLAG_AT]),
        u32(me.counter),
        new Uint8Array(16), // AAGUID: none, as `attestation: "none"` reports it
        new Uint8Array([0, credId.length]),
        credId,
        cose,
      );
      const att = cbor(new Map([["fmt", "none"], ["attStmt", new Map()], ["authData", authData]]));
      return { cred_id: b64u(credId), client_data: b64u(me.clientData("webauthn.create", challenge, origin)), attestation: b64u(att) };
    },
    async assert(challenge, origin, { rpId } = {}) {
      rpId = rpId || new URL(origin).hostname;
      if (me.counter) me.counter++;
      const authData = cat(await sha256(new TextEncoder().encode(rpId)), new Uint8Array([flags]), u32(me.counter));
      const cd = me.clientData("webauthn.get", challenge, origin);
      const sig = new Uint8Array(await subtle.sign({ name: "ECDSA", hash: "SHA-256" }, keys.privateKey, cat(authData, await sha256(cd))));
      return { cred_id: b64u(credId), client_data: b64u(cd), auth_data: b64u(authData), signature: b64u(der(sig)) };
    },
  };
  return me;
}

// Register a passkey for the device on `send`'s link and unlock it — what a phone does once at
// pairing and then on every step-up. `waitFor(pred)` resolves with the first matching message.
export async function enrolAndUnlock(send, waitFor, pk, origin = "https://wado.test") {
  send({ type: "passkey_begin", purpose: "register" });
  const c1 = await waitFor((m) => m.type === "passkey_challenge" && m.purpose === "register");
  send({ type: "passkey_register", ...(await pk.register(c1.challenge, origin)) });
  await waitFor((m) => m.type === "passkey_state" && m.state.registered);
  return unlock(send, waitFor, pk, origin);
}

export async function unlock(send, waitFor, pk, origin = "https://wado.test") {
  send({ type: "passkey_begin", purpose: "unlock" });
  const c = await waitFor((m) => m.type === "passkey_challenge" && m.purpose === "unlock");
  send({ type: "passkey_unlock", ...(await pk.assert(c.challenge, origin)) });
  return waitFor((m) => m.type === "passkey_state" && m.state.unlocked);
}
