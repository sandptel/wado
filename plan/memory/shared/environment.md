# 00 — operating environment

**Read this first.** Each entry below cost real time to discover.

## RTK rewrites shell commands and filters output

A hook rewrites commands through `rtk`. For inspection commands this **silently truncates or
empties the output** — `ps` returned *nothing* while the daemon was running, and that was
reported to the user as "everything is dead". It was not.

**Use `rtk proxy "<command>"` for anything whose exact output matters**: `ps`, `grep` on a
log, `ls`, `cat /proc/...`, `sed -i`.

## The daemon must be a release build

`./target/debug/wado` cannot meet the latency target and **does not fail like a build
problem** — it looks like a network or encoder fault: 100–200 ms `write_sample` stalls, a
jitter buffer climbing 19→42 ms, oscillating throughput, fps dipping to 48 at 1080p. The
same config on release: zero stalls, jbuf flat at 13 ms.

A startup `WARN` under `cfg!(debug_assertions)` now says so. **Measurements from a debug
build are not data.**

## Never loose-match a process

`pgrep -f 'pattern'` matches the shell running the command. This killed the working shell
mid-session. Match the exact pid, or `comm` exactly (`$2=="wado"`).

## GPU work needs the Bash sandbox disabled

Sandboxed access to a DRM render node is killed outright and the exit code looks like a crash.

## The nix builder cannot reach crates.io here

`nix build` fails with "Failed to connect to static.crates.io" even though curl reaches it
fine — the *builder's* network namespace is restricted. Use `--option sandbox false`. This is
local, not a property of the flake.

## The planning docs are gitignored

`CLAUDE.md`, `TODO.md`, `WADO_PLAN.md`, `CHALLENGES.md` are in `.gitignore` and have been
since the first commit. Editing them is useful **on this machine only** — the changes never
reach a clone, and a commit message claiming to have updated them is wrong. Unresolved
whether that is deliberate.

## Bringing the rig up: `scripts/rig.sh`

One command for relay + tunnel + daemon; prints the relay URL and the Remote ID. `--daemon`
restarts **only** the daemon, which is what you want after a rebuild — restarting the tunnel
rotates its URL and invalidates whatever the phone is pointed at. `--stop` stops all three.

Three things it encodes that were each learned the hard way:

- **`setsid`, not just `nohup`.** Started from a terminal that later closes, all three die with it.
  That is exactly how the whole rig was found dead at the start of a session on 2026-09-12.
- **`pkill -x`, never `-f`.** `-f wado` matches the shell running the script itself.
- **cloudflared is not on PATH** in this dev shell; it is resolved from the nix store.

Logs go to `/tmp/wado-rig/{daemon,relay,tunnel}.log` — a fixed path, not a session scratchpad,
because a session scratchpad is deleted when that session ends and takes the log with it.

## The rig

**Several daemons now, not one** — a Remote ID is a pool. See `plan/memory/shared/pool.md`.
Logs are `daemon-1.log`, `daemon-2.log`, … and `scripts/watch.sh` takes an instance number
(default 1). The old single `daemon.log` is no longer written by `rig.sh`; a monitor still
pointed at it tails a dead file, which looks exactly like a quiet system.

| Piece | How it runs |
|---|---|
| `wado` daemons | `WADO_RELAY_URL=ws://127.0.0.1:4000`, release build, `WADO_INSTANCES=N` (default 2), logs → `daemon-N.log` |
| `wado-relay` | local, port 4000 |
| `cloudflared` | quick tunnel fronting the relay, `*.trycloudflare.com` |
| Client | GitHub Pages, `https://sandptel.github.io/wado/`, rebuilt on push to main |
| Remote ID | `872-990-894`, persisted in `~/.config/wado/remote_id` |

The daemon writes its own stdout/stderr to the log, so **Chrome's stderr lands there too** —
filter monitors to lines containing `wado` or they drown in Mojo/GCM noise.

## Log levels: a `debug!` you add is silent in the live daemon unless you say otherwise

`init_logging` in `crates/server/src/main.rs` used a single registry-level
`EnvFilter::new("info")` when `RUST_LOG` was unset. That gated **every** layer at once, so a
`debug!` written to answer a question never fired in the daemon the user is actually running —
and silence from a log line that cannot fire is indistinguishable from silence meaning "the
thing never happened". This cost a build cycle on `2026-09-12`.

Fixed by splitting the filters per layer:

| layer | default | why |
|---|---|---|
| terminal (`fmt`) | `info,wado=debug,wado_compositor=debug,webrtc_ice=warn` | wado's own crates at debug so new `debug!` lines work; `webrtc_ice` muted because one teardown emits eight meaningless "Failed to close candidate" lines |
| client log panel (`LogBus`) | `info` | a 200-line ring in front of a human; wado's debug traffic would push anything readable off the top in seconds |

`RUST_LOG` still overrides the terminal filter entirely.

## The shell working proves nothing about video

`W.ptyOpen`/`ptyInput` go through `relaySend` — the relay **WebSocket**, TCP through the
cloudflared tunnel. So do every session verb, the apps list and the stats. Only media rides
WebRTC/UDP.

A viewer who reports "the shell opens fine but there is no picture" has therefore told you the
**control plane is healthy and the media plane is not** — which is a diagnosis, not a
contradiction. Observed `2026-09-14` on a macOS client stuck at `ICE checking`.

## ⛔ ICE has **no TURN**, and CGNAT-to-CGNAT is where that bites

Observed 2026-09-12 06:57–06:58, a session the user started and never got video from:

```
06:57:44.654  pingAllCandidates called with no candidate pairs. Connection is not possible yet.
06:57:44.654  ICE connection state changed: checking
06:57:44.693  relay client: answer sent — 8 candidates (host,srflx)
06:58:40      no sign of a viewer for 45s and WebRTC is not connected — stopping the session
```

**ICE never left `checking`.** 55 seconds, then the watchdog reaped it — the watchdog did its job;
it is not the bug.

The shape of the failure: both offer and answer carry **`(host,srflx)` and nothing else** — there
are no `relay` candidates, because wado configures **STUN only** (`ice.rs`: three STUN servers, no
TURN). srflx candidates are useless when both peers are behind carrier-grade NAT, which is what
the addresses say: daemon public `157.49.114.134` (Jio), browser public `59.89.212.117`, and a
daemon host candidate of `192.0.0.4` — the well-known **464XLAT CLAT** address, i.e. a mobile
IPv4-over-IPv6 network.

**This is the mechanism behind the recurring "stuck at connected to relay" on mobile data.** It is
not a timeout, not the tunnel and not the daemon: two CGNAT endpoints cannot hole-punch, and
nothing in the candidate set can carry the media instead.

Closing it needs a **TURN relay** — either a TURN server, or teaching `wado-relay` to forward media
when ICE fails. The second is already the architecture's stated fallback ("a rendezvous relay
brokers SDP/ICE **and can relay media**") and is not built. Until then, a session between two
mobile networks is expected to fail, and no amount of STUN tuning changes that.

### ⚠️ Verifying a Pages deploy: the wasm name is **not** in `index.html`

`index.html` is ~1.6 kB and references only `/wado/assets/wado-client-<hash>.js`. The wasm
filename lives inside **that** file. A check that greps the index for `wado-client_bg-*.wasm`
finds nothing — and if it is written as

```sh
curl … | grep -o '…\.wasm' | head -1 | xargs -I{} sh -c 'curl … | grep -q SYMBOL'
```

then empty input means `xargs` runs **nothing** and the pipeline exits **0**. The check reports
success without having looked at anything. Cost a false "deployed" claim on 2026-09-12.

The chain is three hops and all three must be followed:

```sh
L=$(curl -s https://sandptel.github.io/wado/ | grep -o '/wado/assets/wado-client-[A-Za-z0-9]*\.js' | head -1)
W=$(curl -s "https://sandptel.github.io$L" | grep -o 'wado-client_bg-[A-Za-z0-9]*\.wasm' | head -1)
curl -s "https://sandptel.github.io/wado/assets/$W" | strings | grep -c SYMBOL
```

**The wasm hash is the deploy's identity.** If it matches the previous build's, nothing shipped —
the JS files are `include_str!`'d into the wasm, so any client change must move that hash.
`gh run list --limit 3` says whether the build is still `in_progress`, which is the usual answer.

This is the same rule the dmabuf verdict taught, applied to a shell pipeline: **a check that can
only report success is indistinguishable from no check.** Assert on the hash changing, not just on
the symbol being found.

### `Failed to close candidate udp6 srflx …` is the **phone's** candidate, not ours

Seen 2026-09-12 on a working session, alongside the daemon's own
`no global IPv6 on this host — ICE gathers over IPv4 only`. The two look contradictory and are
not: `webrtc-ice`'s `delete_all_candidates` closes the **local and remote** lists and logs the
same message for both (`agent_internal.rs:659` and `:676`). A `udp6 srflx 2409:…` line is the
mobile client's address, gathered on its side.

The decisive argument is **not** the `/proc` read — that only describes this instant. It is that
`set_network_types(vec![Udp4])` was applied to the engine, so **a local udp6 candidate cannot
exist**; therefore every `udp6` line in a teardown is remote. A remote candidate is routinely of
type `host` (the phone's own mobile address), so `udp6 host 2409:…` is not evidence of a local
address either. **Do not re-diagnose this as "the IPv4 restriction is not taking effect".**

### ⚠️ `has_global_ipv6()` is evaluated **once per daemon start**, not per connection

`build_webrtc_api()` is called once in `relay_client::run` — *"Built once, survive reconnects"*
(`relay_client.rs:126`) — so `build_setting_engine()` and its IPv6 probe run at startup and the
`API` is reused for every later peer connection. Consequences:

- The `no global IPv6 on this host` line appears **once in the whole log**, not once per session.
  Its absence on later sessions means nothing.
- If the host **gains or loses** global IPv6 after the daemon starts — routine on a tethered
  phone — the daemon keeps its startup decision until restarted. A dual-stack host that was v4-only
  at boot never gathers v6, and vice versa.

Restart the daemon after a network change if ICE behaviour looks wrong.

The `webrtc_ice=warn` entry in the terminal filter does *not* silence these — it raises the
threshold from info to warn, and these lines are warns. They arrive eight at a time on every
teardown, so a log monitor should exclude `webrtc_ice` explicitly rather than rely on the filter.

**Per-lane detail needs no new code.** A tracing target *is* the module path, so
`RUST_LOG=wado_compositor::headless=debug,wado_compositor::input=trace` already works. Do not
build a logging abstraction for this.

## A verdict that only logs "yes" reads the same as nobody looking

Applied to the dmabuf question (`headless.rs` / `handlers/dmabuf.rs`): a session logs
`dmabuf path is live` on its first successful import, and `dmabuf path unused this session` at
stop when there was none. Both branches speak. The alternative — logging only success — is how
the question sat unanswered while the only way to check was counting `/proc/<pid>/fd` entries
during a session that lasts twenty seconds.

## ⚠️ Check the WAN link before debugging a connection failure

On `2026-09-12` an afternoon went into "the phone cannot connect", through three wrong causes
(missing STUN servers, IPv6 gathering, the pinned UDP port range) before the link itself was
measured. It was the link:

| target | loss |
|---|---|
| router `192.168.1.1`, ICMP | **0%** |
| `1.1.1.1`, ICMP | **27%** |
| `1.1.1.1`, UDP/53 | **50%** |
| three separate STUN servers, UDP | **45–50%** |

Clean to the router, catastrophic beyond it — the loss is upstream of the LAN, in the WAN
connection. Nothing in wado can fix that, and at 50% loss the media stream is unusable even if
ICE completes. A single STUN request with no retry fails half the time, which is exactly what
"answer sent — 2 candidates (host)" is.

**Measure the link first, in this order** — thirty seconds, and it would have saved the whole
detour:

```
ping -c 15 -q <router ip>        # clean → the LAN is fine
ping -c 15 -q 1.1.1.1            # lossy → the WAN is the problem, stop debugging wado
```

Tethering the host to the phone is the known-good path: it puts both ends on one network and
bypasses the WAN entirely. Every session that worked on `2026-09-12` was tethered; every one
that failed was on WiFi.

## The cloudflared quick tunnel is the weakest link in the rig

`DEFAULT_RELAY` in `crates/client/src/state.rs` is a **quick tunnel URL, and it changes every
time `cloudflared` restarts**. When it is stale or the tunnel is down, a phone gets
*"relay reachable but no daemon answered for this Remote ID"* — which reads like a daemon
problem and is not one. Check the tunnel before the daemon:

```
curl -s http://127.0.0.1:4000/health     # local relay — should be {"servers":1,...}
curl -s <DEFAULT_RELAY>/health           # what the phone actually hits
```

Local ok + tunnel not ok ⇒ it is the tunnel, every time.

`cloudflared` is **not on PATH**; it runs from its nix store path. Killing it without that path
to hand means it cannot be restarted.

On a lossy WAN it will not come up at all: its bootstrap needs to resolve
`regionN.v2.argotunnel.com` and POST to `api.trycloudflare.com`, and at 50% packet loss both
time out — `hard_fail=true`, then exit. `--protocol http2 --edge-ip-version 4` avoids QUIC over
UDP and is the better flag set here, but it does not rescue a link this bad. A tunnel that is
created but shows **530** or **1033** is registered without an edge connection: same cause.

## Panic containment: what is guarded and what was not

Audited `2026-09-12` after a relay panic wedged every connection in a room.

| surface | before | now |
|---|---|---|
| compositor command source | `catch_unwind` | unchanged |
| compositor input source | `catch_unwind` | unchanged |
| compositor render timer | `catch_unwind` | unchanged |
| **wayland client dispatch** | **none** — a client request could kill the daemon | `catch_unwind` |
| **relay-client connection** | **none** — a panic killed the thread forever, silently | `catch_unwind`, treated as a lost connection |

The pattern worth remembering: **a panicking tokio task is not a crash.** The process stays up,
`/proc` looks healthy, other rooms keep working — and one subsystem is dead with nothing saying
so. Panics now route through `tracing` (`panic_log.rs` in both binaries) so they are timestamped
and greppable instead of bare stderr.

Two teardown rules that came out of the same audit: cleanup written at the end of a function
does not run on an unwind (use an `AbortOnDrop` guard), and a long-lived resource must never
depend on a *single* event to be released — both transports now have a `viewer_watchdog` that
stops a session after 45 s with no viewer by any route.

## Killing a monitor's pid orphans its pipeline

`scripts/watch.sh` is `tail -F … | sed -u … | awk '…'`. Killing the **bash** pid leaves `tail`,
`sed` and `awk` running, reparented to init, still appending to the same log.

Cost, `2026-09-13`: a patched `watch.sh` was started alongside the **unpatched** awk from the
previous generation, both writing to `watch-mine.log`. The new rule looked broken — it was in
the file, the file predated the process, and it worked standalone — because a second, older awk
was producing the lines. Two generations of orphans were found (`360540` from the session before
that one).

**Restart it by process group, not by pid.** `rig.sh` and the monitor are started with `setsid`,
so each is its own group leader and `kill -- -<pgid>` takes the whole pipeline:

```sh
PGID=$(ps -o pgid= -p "$PID" | tr -d ' ')
kill -- -"$PGID"
```

Same shape as the bug `proc::terminate` fixes for session applications — kill the group, because
the thing you launched is rarely the only thing running.

**And check for writers, not for the process you remember starting.** The decisive test was
scanning `/proc/*/fd` for the log, which found the orphans immediately; `ps | grep watch.sh` had
shown exactly one process and looked reassuring.

## `gh run list --limit 1` races the push you just made

Watching a deploy with `gh run watch $(gh run list --limit 1 ...)` reported **CI OK for a run
that finished before the push**, and the three-hop check then read the *previous* wasm hash —
so a deploy that had not happened looked verified. GitHub had not created the new run yet when
the query ran.

Match the run to the commit, never to recency:

```sh
gh run list --limit 5 --json databaseId,headSha,status   # find the row whose headSha == HEAD
```

The wasm hash changing is the only proof the deploy landed. When it has not moved, the deploy
has not happened — regardless of what the CI check said.

## A same-LAN client that cannot complete ICE: suspect the access point, not wado

`2026-09-14`: a macOS client on the *same WiFi* as the host stalled at `ICE checking` in every
browser, repeating offer→checking→never-connected every 12 s, while the PTY shell worked
perfectly (see above — that is the WebSocket).

What was ruled out from the host side, and is worth ruling out in this order because each is
one command:

```
ip -4 -o addr show          # the host's real address — 192.168.1.239/24, NOT a tethered CLAT
nft list ruleset            # empty
iptables -S INPUT           # empty
ss -lun                     # sockets bound in the pinned 50000-50100 range, on the LAN address
```

All clean, so the host was reachable and advertising a usable host candidate. What remains is
the path: **AP / client isolation** on the access point blocks station-to-station traffic, which
kills host↔host candidates, and srflx↔srflx then needs NAT hairpin which most routers refuse.

The tell is that a *phone on mobile data* connects fine while a *laptop on the same WiFi* does
not — the phone arrives via the public srflx address and never needs station-to-station.

Decide it with one command **on the client**: `ping <host LAN ip>`. No reply ⇒ isolation, and
nothing in wado can fix it — that path needs TURN.

Do not reach for the mDNS warnings when diagnosing this. `Failed to discover mDNS candidate
<uuid>.local` appears microseconds after `ICE connection state changed: closed` and belongs to
the **previous** agent tearing down, exactly like the `Failed to close candidate udp6 srflx`
lines above. Both are teardown noise.

## The relay never pinged its clients, so idle sessions re-joined every 1-2.5 minutes

Found `2026-09-14` while stress-testing multiple devices. Once media is flowing, the relay
WebSocket carries **nothing** — video and input are on WebRTC — so the signalling socket sits
idle, and the **cloudflared quick tunnel closes an idle connection**. The client reconnects,
which costs a fresh room, a fresh offer/answer, a black frame, and four ICE ports on the daemon
(I14).

It read as flaky Wi-Fi. What it actually looked like in the relay log was one client joining and
disconnecting at 16:29, 16:34, 16:39, 16:40, 16:42 — and in the daemon log, **two** `Connected`
events about 2 s apart on each cycle.

`RelayMsg::Ping`/`Pong` already existed, and `js/relay_link.js` already answered `ping` with
`pong`. **Only the relay's sender was missing** — so the fix is relay-side only and needs no
client deploy. `KEEPALIVE` is 30 s in `signaling.rs`, sent into the room's own inbox so it goes
through the single task that owns the socket rather than a second writer.

⚠ **Deployed 22:14, not yet confirmed.** Confirming it needs a device left **idle and untouched
for ~5 minutes** with no re-join in the relay log; the measurement taken straight after was
confounded by someone actively reconnecting. Until that is done, treat this as reasoned rather
than verified.

A `pong` from a client is swallowed at the relay — forwarding it to the daemon would get an
"unknown message" `SessionError` back, because the daemon has no reason to know about this
socket's liveness.
