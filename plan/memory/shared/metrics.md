# metrics — what to collect when debugging

The minimum set that distinguishes *where* a fault is. Collecting fewer means guessing;
every wrong diagnosis in this project came from missing one of these.

## The rule

**Each metric must separate two hypotheses.** A number that goes up when anything is wrong
tells you nothing. Below, "separates" names what the metric actually rules in or out.

---

## Producer side (server)

| Metric | Source | Healthy | Separates |
|---|---|---|---|
| `tick` (render tick ms) | `browser: latency` | ≤ budget (8.3@120, 16.7@60) | **Producer vs transport.** Above budget = frames aren't being made fast enough; no network fix applies |
| `capture` ms | `browser: latency` | 0.2 (dmabuf) / 2–3 (software) | Zero-copy path present or lost |
| `encode` ms | `browser: latency` | 2.6–2.9 hw / 7–9 sw | Encoder tier cost |
| `pipeline selected tier=` | startup | `vaapi-dmabuf` | Which tier actually ran — **not** what was requested (`backend=`) |
| `worst_queue_ms` | `pump: queue wait` | 0–7 | **Pump backlog.** Frame waited between compositor and pump |
| `write_sample stall` count | stall warn | 0 | Transport write blocking |
| `bitrate_kbps` | `session active` | per the ladder | Config actually applied vs requested |

## Consumer side (browser, echoed into the server log)

| Metric | Source | Healthy | Separates |
|---|---|---|---|
| `fps` vs requested | `browser: stats` | within 1–2 | Producer keeping up |
| `jbuf` **as a range** | `browser: stats` | flat, ≤15 | **Even vs uneven delivery.** A *climbing* buffer is the signature; the absolute value is secondary |
| `framesDropped` | `browser: stats` | 0, and **not growing** | Client-side decode/display fault. Cumulative — read the delta |
| `lost` | `browser: stats` | 0 | Real packet loss. Can reset to `-1`/negative — a counter reset is **not** loss |
| `rtt` | `browser: stats` | — | **Which network leg.** `0` = LAN; never compare a LAN row to a cellular one |
| `decode` ms | `browser: latency` | ~2.5–2.8 | Client decoder cost; tier-independent |
| `buf` ms | `browser: latency` | 12–13 | Playout buffer |

## Not yet obtainable

- **`input(rt)` prints `?`** — input latency has never actually been read. Needs Debug →
  latency enabled client-side.
- **Glass-to-glass** is not measured anywhere and cannot be derived: browser and server
  clocks are unsynchronised. **Never add browser and server numbers together and call the
  result end-to-end.**

## Diagnostic table

| Symptom | tick | qmax/stalls | jbuf | drop | Reading |
|---|---|---|---|---|---|
| fps below requested | **over budget** | clean | flat | 0 | Producer-bound — encoder or capture |
| stutter, fps oscillating | ok | **stalls + high qmax** | **climbing** | 0 | Pump/transport bottleneck |
| stutter, everything clean | ok | clean | flat | **growing** | Downstream of the server — client decoder |
| pixelation on motion | ok | clean | flat | 0 | Bitrate starvation, or error propagation from an earlier drop |
| stutter only sometimes | ok | **spikes** | ok | 0 | Host contention — is something compiling? |
| **sluggish, picture fine** | ok | clean | **high and flat, was low** | 0 | Jitter buffer ratcheted on an rtt spike and never drained — check `jbuf` first |

## Which side is at fault — the three discriminators

Established on the roaming run of `2026-09-12`, each after getting it wrong once.

| Reading | Says | Because |
|---|---|---|
| loss rate high | **the path** | nothing on either machine is wrong |
| loss ~0, throughput far under target, **fps also down** | **the sender** | it was never sent. The fps clause is load-bearing: a still screen legitimately encodes to ~60 kbps and without it reads as a dead server |
| everything arrived, frames dropped after arrival, `dec` over budget | **the receiver** | the phone could not keep up |
| `write_sample` stall with **`runq ≈ took`** | **ours — CPU starvation** | the thread was runnable and got no CPU |
| `write_sample` stall with **`runq ≈ 0`** | **the link — congestion** | the thread was blocked on the socket. Compositor tick-shedding alongside it is the *correct response*, not a second fault |

## ⛔ `availableIncomingBitrate` is **not** spare capacity

Chrome tracks the *received* rate with it whenever nothing is congested. A static screen
sending 600 kbps therefore reports a "600 kbps link", and comparing that against the encoder
target accuses the network continuously while nothing is wrong — which is exactly what the
health verdict did for ~50 s on `2026-09-12` before the rule was gated on actual harm.

It is evidence **only alongside a symptom** (fps under target, or loss). On its own it is a
display figure.

Related: on a fresh connection it ramps (observed 500 kbps → 2.6 Mbps over the first minute).
That is BWE converging, not a link improving. **Every rule here reads a one-second rate, so the
first few seconds of a connection are ramp and must not be judged at all.**

## Instrumentation traps

- **Never log only above a threshold.** It censors the sample and fabricates patterns — see
  [`verification.md`](verification.md). Record every frame; report percentiles.
- **Attribute session-scoped lines carefully.** `pipeline selected` is logged *before*
  `session active`.
- Cumulative counters (`framesDropped`, `lost`) need deltas, and can reset.
- Chrome's stderr shares the daemon log — filter on `wado` or drown in Mojo/GCM noise.

## `ICE state=Failed` means two different things — check what came before it

A `Failed` that follows a `Connected` is a **viewer that went away** (screen lock, WiFi sleep,
walked out of range). A `Failed` with no preceding `Connected` is a negotiation that **never
established**. They need opposite responses, and a monitor that labels both "failed" sends you
after the wrong one — observed `2026-09-14`, where a device that had streamed for 2.5 minutes
was reported as never having connected.

The drop is recognisable by its timing, which is just the configured ICE timeouts in
`webrtc_settings.rs` counting down:

```
16:35:04  state=Connected  →  viewer connected via WebRTC
16:37:26  state=Disconnected      ← the device stopped answering (15 s timeout)
16:37:56  state=Failed            ← exactly 30 s later (the failed timeout)
```

`Disconnected` then `Failed` 30 s apart is a normal departure, not a fault. The session survives
it — `VIEWER_GRACE` is what decides, not the peer-connection state.

`scripts/watch.sh` and the run monitor track whether a viewer ever reached `Connected` on that
daemon and label the two cases differently.

## Fingerprint devices by their offer's ICE candidate count

`offer received — N candidates` is the cheapest way to tell devices apart in a log, and with a
daemon pool it is essential: failures land on whatever daemon was free, so **per-daemon failure
counts are not per-daemon facts**. On `2026-09-14`, 27 failures on one daemon and 29 on another
were one device with a dead media path; both daemons carried media fine minutes later.

Counts seen on this rig: 6 and 8 connect in 1-2 s; 15 never established media in ~38 attempts.
A count of 4 `(host)` with no `srflx` means STUN timed out — only LAN peers can reach that one.
