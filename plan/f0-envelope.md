> **Status `2026-10-04`: built.** What differs from this plan is recorded in the Decision Log entry (client_key stays the random id; finish in the clear; SRI instead of CSP; no rollback switch).

# F0 — end-to-end envelope over the relay (plan, for review)

Decision Log `2026-10-04`. Goal: the relay (or the tunnel in front of it) can neither read nor
inject a daemon↔client message. Direct mode (localhost HTTP) is out of scope and untouched.

## Wire (new `crates/protocol/src/envelope.rs`, types only)

All handshake and sealed frames are ordinary relay-forwarded text frames — the relay forwards
unknown types unread (`relay/src/signaling.rs:2`), so **no relay change**.

```
client → daemon  e2e_hello   { v:1, eph_c }                         after join_accepted
daemon → client  e2e_reply   { eph_d, host_pk, sig_d }              sig_d = Ed25519(host, H(T1))
client → daemon  e2e_finish  { dev_pk, sig_c, pair_mac? }           sealed already (k_c2d)
both             sealed      { n, c }                                AES-256-GCM, n = counter

T1 = "wado-e2e-v1" ‖ room_id ‖ client_key ‖ eph_c ‖ eph_d ‖ host_pk
T2 = T1 ‖ dev_pk           sig_c = Ed25519(device, H(T2))
pair_mac = HMAC-SHA256(pair_code, H(T2))      — pair code never leaves the device
keys     = HKDF-SHA256(X25519(eph), salt=H(T1)) → k_c2d, k_d2c (32 B each)
nonce    = 4 zero bytes ‖ u64 counter (BE); a counter that is not exactly last+1 closes the room
```

Plaintext forever (relay-addressed): `ping pong waiting taken_over join_accepted join_denied
leave peer_* seat_hold occupancy register registered`. Everything else: sealed or dropped.

## Daemon — `crates/server/src/e2e/` (one job per file)

| file | job |
|---|---|
| `host_key.rs` | load or create `<config>/wado/host_key` (Ed25519 PKCS#8, 0600); `pk_hash()` for the QR |
| `handshake.rs` | state machine Hello→Reply→Finish; verifies `sig_c`, `client_key == hash(dev_pk)`, `pair_mac` |
| `seal.rs` | `Sealer`/`Opener`: AES-256-GCM, strict counter |
| `mod.rs` | `Envelope` — the per-room state the relay loop holds |

Hooks, nothing else in `relay_client.rs` changes shape:
- **Outbound writer** (`out_rx → ws`): relay-addressed types pass; the rest are sealed, or
  **held** until the handshake finishes (bounded queue, dropped on room change).
- **Inbound parse** (`relay_client.rs:980`): `e2e_*` → handshake; `sealed` → open → existing
  `match msg`; any other non-relay frame → dropped with a WARN (that is the injection test).
- **Gate** (`gate.rs`): the trust list keys on `hash(dev_pk)`. `PeerConnected` still decides
  from the relay's claim (for the parking UX only); authority moves to `Finish` — claim and
  key disagree, or the key is not trusted/approved → close the room. Pair codes are checked by
  MAC at `Finish`, not by string at `PeerConnected`.
- **Trust list** gains columns, old lines still parse:
  `<client_key>\t<name>\t<dev_pk|legacy>\tpinned=0|1\tfiles=none|ro|rw`.
  Legacy UUID line + first `Finish` with a matching claim → rewritten with that key,
  `pinned=0` (TOFU, one connection's window). QR-paired → `pinned=1`.
- `wado qr` (`cli/qr.rs`) appends `&hk=<b64url sha256(host_pk)>`.

Crypto: `ring` 0.17 only (already in the lockfile — no new crate).

## Client — `crates/client/src/js/` (WebCrypto, no WASM crypto)

| file | job |
|---|---|
| `e2e_keys.js` | device key: `generateKey(Ed25519, extractable=false)`, stored in IndexedDB; `client_key = b64url(sha256(pk))` replaces the random UUID (`relay_link.js:108`) |
| `e2e_pin.js` | host pins per Remote ID in localStorage; `hk=` from the link pins, else TOFU + "unpinned" mark; mismatch → hard stop with a visible warning, never a silent retry |
| `e2e_handshake.js` | Hello/Reply/Finish; X25519 ephemeral via WebCrypto |
| `e2e_seal.js` | AES-GCM seal/open, counters, an ordered promise chain so async sealing keeps send order |

Hooks: `W.relaySendMsg` seals (queues until ready); `ws.onmessage` opens `sealed`, drops other
non-relay types. `&pair=<secret>` in the WS URL becomes `&pair=1`; the secret stays in
localStorage until a successful `Finish`.

CSP: `<meta http-equiv="Content-Security-Policy">` in the Dioxus index — `default-src 'self';
script-src 'self' 'wasm-unsafe-eval'; connect-src 'self' wss: https:` (relay URL is
user-editable, so `wss:` not a single host — narrowing it is a follow-up once the relay host is fixed).

## Order, each step with its check

0. **Probe**: does the user's Android Chrome do WebCrypto Ed25519 + X25519? (Chrome ≥ 137.)
   One-line check in the console before any code. If not: stop and decide.
1. `envelope.rs` + `e2e/seal.rs` + `handshake.rs` with a **shared test vector** (fixed keys →
   fixed bytes), checked by `cargo test` and by a node script against the JS side.
2. Daemon hooks behind `security { e2e #true }` (on by default; `#false` exists only for the
   rollback window and logs a loud WARN).
3. Client modules + hooks. `dx build`.
4. Trust-list migration + `wado qr` `hk=`.
5. e2e: existing relay harness + a **hostile-relay test**: inject plaintext `session_launch`
   → ignored; flip one ciphertext byte → room closed; swap `host_pk` → client hard-stops;
   replay a sealed frame → room closed; relay-forged `client_key` of a trusted device → closed.
6. Release build, swap the daemon pool, push the client, re-pair the phone by QR, verify.

## Costs / risks, said plainly

- Every device re-runs a handshake per join: one RTT through the relay (~+30–80 ms to connect,
  not to latency once streaming — media is P2P and untouched).
- Existing devices convert by TOFU once; anything that wants files re-pairs by QR anyway.
- A device whose IndexedDB is cleared gets a new key = a new device (re-pair). Intended.
- Not covered: a compromised client deploy (origin trusted by decision 5).
