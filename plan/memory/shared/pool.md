# Daemon pool — one Remote ID, several wado processes

Landed `2026-09-14`. Supersedes "one viewer at a time (iter 1)" in `crates/relay/src/room.rs`.

## The shape

A Remote ID names a **pool**, not a process. Any number of `wado` daemons register under it;
the relay gives each joining client one of its own. Two devices therefore get two genuinely
independent sessions — separate compositors, applications, encoders, Wayland sockets.

| piece | key |
|---|---|
| `registry` | **instance id** (uuid minted per registration), value carries `remote_id` |
| `rooms` | **instance id** — not remote id |
| assignment | preferred (`?instance=`) **if free** → first free → refuse |

`WADO_INSTANCES=N scripts/rig.sh` sets the pool size; default 2. Logs are `daemon-N.log`.

**A daemon can be added to a live pool with no restart** — start another `wado` with the same
`WADO_REMOTE_ID` and it registers itself. Done on `2026-09-14` at 21:16, 2→4, while two
sessions were streaming. This is the pool's best property; do not replace it with a scheme
that needs a restart to change N.

## ⛔ Never take a room from a live client

Tried on `2026-09-14` and **reverted the same hour**. Last-writer-wins displacement looked like
the fix for "a stale tab blocks the next device" and instead produced a steal-war: each evicted
client reconnects, evicts the other, forever. **132 joins in two minutes.**

This is `issues.md` **I17**, which had already recorded the same failure at an 18 s period. The
client even carries a guard for it — `OCCUPIED_RE` in `js/relay_link.js` matches the relay's
denial wording and backs off hard — and displacement bypassed that guard entirely by never
denying. A fix that removes the condition a guard tests removes the guard.

Contention is answered with a *different* daemon, or a refusal. Never by stealing — including
for the same device reclaiming its own instance, which two tabs of one browser would turn back
into the same war.

## ⛔ Daemons in a pool must not share the UDP port range

The bug the pool introduced, found and fixed the same hour on `2026-09-14`.

`webrtc_settings.rs` pinned **every** `wado` process to `50000-50100`, so four daemons put the
ICE sockets of four independent sessions into one 101-port window. Candidates were advertised
on ports a *sibling process* held. Every session went `Checking → Failed`, and **each daemon's
own log looked perfectly healthy** — offer received, answer sent with 8 candidates, no error.
The only visible symptom was the client stuck at "connected via relay".

The tell that it was self-inflicted: single-daemon sessions had been reaching `Connected` in
under a second an hour earlier, on the same machine and the same network.

**All four daemons confirmed healthy.** Every one of d1-d4 has carried media. A per-daemon
failure count says nothing about that daemon.

**Confirmed working `2026-09-14` 21:42** — two devices streaming at once on one Remote ID:
d2 a phone at 1080x2422@90, d4 a laptop at 1670x1080@60, both `Connected`, `rooms:4 servers:4`.

**But slicing was not what fixed ICE, and the collision theory was wrong.** Recorded because a
wrong conclusion left standing costs more than the bug: after slicing, daemons *still* failed —
d1 27 times, d2 29 times — and then d2 connected first try for a different device. A daemon that
had "failed 29 times" was healthy all along. Two further theories were measured and killed:

| theory | measurement that killed it |
|---|---|
| port-range collision between pool members | `ss -lunp`: every pid bound only inside its own slice |
| I14 port exhaustion recurring | **5** bound UDP sockets per daemon, not ~100; answers still carried 8 candidates |
| a broken/poisoned daemon (d1, d2) | **both** later connected media in under a second for another device — d2 at 21:42, d1 at 22:00 after its 27 failures |

What the failures actually were: **one device whose media path does not work at all**, retrying
across whatever daemon was free and leaving failure counts behind it. Tell devices apart by
**ICE candidate count in the offer** — that is the cheapest fingerprint in the log. Here the
6-candidate device connected; the 15-candidate device never once established media in ~38
attempts, on four different daemons, while this host's link measured 0% loss to both router and
WAN. That is a path problem between the two endpoints (AP isolation / no TURN), not a wado bug.

⚠ **A device that cannot connect occupies a pool slot while it retries** — the failing one drove
`pool_busy` to 4 of 4 and started refusing working devices. Pool size is not just a resource
ceiling, it is also a retry-storm blast radius.

Slicing is kept regardless: sharing one 101-port window across N daemons is wrong on its face,
and I14 means the leak rate scales with negotiations.

The change: `WADO_UDP_SLICE=n` gives this process `50000 + n*100 ..= +99`, ceiling raised to
`50400`. `rig.sh` passes `n-1` per instance. Slice 0 is the default, so a lone daemon keeps its
old range and its old firewall rule. `build_setting_engine()` is called once per process (relay
mode *or* direct mode, never both — they are selected in `main.rs`), so one slice per process
holds. Verify after any pool change — it is one command and
it is the difference between working and a failure that logs nothing:

```sh
for n in 1 2 3 4; do grep -o 'UDP ports [0-9-]*' /tmp/wado-rig/daemon-$n.log | head -1; done
```

Four distinct ranges, or ICE will fail for reasons nothing in the logs will name. Firewall rule
is now `udp 50000:50400`.

**Raising `WADO_INSTANCES` past 4 needs `WEBRTC_UDP_PORT_MAX` raised too** — a slice past the
ceiling logs an error and falls back to slice 0, which collides by definition.

## The refusal wording is load-bearing

`signaling.rs` must keep the phrase **"already has an active connection"**. The client matches
it to tell "pool full" (back off hard) from "no daemon online" (retry fast).
`scripts/relay-link-check.mjs` reads the relay source and fails if it moves. Both branches of
the marker speak: success logs which daemon and the occupancy, refusal logs the same numbers
and how to raise the limit.

## Cost

| | |
|---|---|
| idle daemon | **152 MB RSS** (measured, `2026-09-14`) |
| relay | 5.8 MB |
| rig host | 20 cores, 27 GB |

`build()` runs before mode selection in `main.rs` — display, `Wado::new`, Wayland socket and
xkb keymap are all paid at startup, viewer or no viewer. "Idles with no compositor" in
`CLAUDE.md` describes the *session*, not the process.

The real ceiling is concurrent **hardware encode sessions**, not RAM or cores. Not yet measured
— do that before raising N much past 4.

## Still open

- **Stickiness is unverified.** `?instance=` is written in `js/relay_link.js` but not deployed
  to Pages, so every join today is `assigned`. Until it ships, a device that reloads may land
  on a different daemon and see an empty desktop instead of its own.
- `scripts/watch.sh` keeps per-session state (`tgtfps`, `last_drop`). It now takes an instance
  number and defaults to `daemon-1.log`. Watching several sessions needs one per instance —
  one awk over two sessions attributes one device's numbers to the other.
