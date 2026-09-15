# compositor — session and process lifetime

## Launched apps die with their session (since 2026-09-11)

**The bug:** apps inherited the daemon's own process group, so `child.kill()` reaped exactly
one pid. `sh -c chromium` execs into chromium, which forks a zygote, a GPU process and a
renderer per tab — all reparented to init and kept running, still holding the machine's
audio device. Observed live: every Chrome process sat in pgid == the daemon's pid.

**The fix** (`compositor/src/proc.rs`): each app spawns as its own process-group leader
(`process_group(0)`) and is signalled by group — SIGTERM, 300 ms grace, then SIGKILL.
SIGTERM first because SIGKILL cannot be caught, and an app that cannot catch it cannot tear
down its own children either.

**Ceiling:** an app that calls `setsid` for itself leaves the group and escapes. Observed
with `chrome_crashpad`. Upgrade path is a per-session cgroup or `systemd-run --scope`.

## Signals run the shutdown

There was **no signal handler anywhere in the tree** — SIGINT/SIGTERM killed the daemon
outright and `stop_session` never ran, so every launched app survived its session.

A calloop `Signals` source (signalfd) now stops the session before the loop stops. It lives
in the compositor's `build()`, not the server's `main`, so calloop/Smithay types stay on
their own side of the crate boundary. A **signalfd, not a flag** — the loop is asleep in
`poll` when the signal lands, and a flag only gets read once something else wakes it.

Enabled by naming `calloop` in the compositor's `Cargo.toml` purely to turn on its `signals`
feature, which Cargo unifies onto smithay's re-exported copy.

**Verified live**, not just in tests: `signal received — stopping session signal=SIGTERM` →
`compositor session stopped` → no survivors in the group.

## The PTY shell

`server/src/pty.rs`, `portable-pty`. A login shell on a real terminal; xterm.js interprets
the ANSI. `WAYLAND_DISPLAY` is inherited so GUI apps started there appear on the stream.

- Reads are **blocking** — no async PTY interface — so the reader is a dedicated thread
  handing text to tokio through a channel.
- Drop the slave fd after spawning or the master never reports EOF.
- Output crosses as **UTF-8 text**, with the trailing partial character held back. Decoding
  each read independently corrupts that character *and* the next (the mojibake in
  box-drawing and non-ASCII prompts). Invalid-rather-than-incomplete bytes are consumed with
  U+FFFD, or they stall everything behind them forever.
- Resize is not optional — a shell never told its size wraps at the wrong column.
- Dropping the `Pty` kills the shell; the kernel's SIGHUP takes its jobs with it.
- **Ceiling:** no exit code on `PtyExit` — the thread that notices the exit does not hold the
  child handle.

## Still untied to session lifetime

`server/src/exec.rs` spawns into the daemon's own group and never registers the child, so a
GUI app started from the old console shell outlives its session. Deliberately not "fixed" by
giving it its own group — that would stop a terminal Ctrl-C reaching it, which is worse.
**The PTY has now replaced it; the right resolution is deleting the exec path.**


---

## Resource governance — apps get a CPU weight, and Chrome partly escapes it

Since 2026-09-11, `proc::spawn` puts each session application in a transient
`systemd-run --user --scope` at `CPUWeight=50` (default 100), overridable with
`WADO_APP_CPU_WEIGHT`; `0` restores the old plain spawn. The scope is for **resources only** —
the process group is still what governs lifetime, and is still what reaps a browser's
helpers. Two mechanisms, two jobs, deliberately not merged.

⚠️ **`-p=CPUWeight=N` is not valid `systemd-run` syntax** — it wants `--property=CPUWeight=N`.
The wrong form fails the scope silently and the app runs unconstrained. There is a test
asserting the weight *landed* in the cgroup, because a test that only asserted the command
ran would pass via the fallback path.

⚠️ **Chrome partly escapes the weight.** Measured live: 16 Chrome processes sat in
`wado-app-*.scope` at 50, while Chrome's main process re-parented itself into
`app-com.google.Chrome-*.scope` at 100. Any app may ask the user manager for its own scope,
so *lowering applications is escapable by design*. wado's own scope `cpu.weight` was verified
writable by its own uid, so **raising wado instead** is the escape-proof alternative — with
the trade-off that it outranks everything else on the user's desktop too.
See `plan/optimisation.md` O1; do not build it before `runq_ms` says the stalls are CPU at all.

## Three lifetimes, not one (`2026-09-13`, branch `graceful`)

`session_active` was a single flag covering three things with completely different natural
lifetimes, and every graceless behaviour followed from that:

| | holds | lives for |
|---|---|---|
| **desktop** | display, `space`, `seat`, `app_processes`, window state | until the user says stop |
| **pipeline** | renderer, encoder, capture, damage tracker, `Output`, render timer | as long as the current *shape* is right |
| **viewer** | peer connection, relay socket, strain/shed state | seconds to minutes |

Verbs now match:

- `ViewerAttached(bool)` — pauses the render tick when nobody is receiving. **0.0% of a core
  while detached**, whatever the desktop is doing. It is also the single owner of the per-viewer
  reset (congestion window, strain flag, shed divisor, keyframe), which used to live in two
  places that drifted.
- `Reconfigure` — rebuilds encoder, capture, `Output`, damage tracker and render timer; keeps
  the renderer, the dmabuf global, the windows and the processes. **6–50 ms.**
- `Stop` — means only what a human asked for.

**Pausing made the grace affordable.** `VIEWER_GRACE` went 45 s → 600 s, and the old 45 s was
never about the user's networks — it was about a session with no viewer still encoding 90 frames
a second for nobody.

### Replacing an `Output` is not free, and it is not the compositor's error when a client dies

Invariant #8 forces a fresh `Output` on resize. Two things learned doing it:

1. **Skip it when the shape did not change.** A bitrate-only reconfigure must not touch the
   output at all.
2. **`disable_global` first, `remove_global` five seconds later.** Removing a global out from
   under a bound client is a protocol error on its next request, which disconnects it, which
   exits it. `global_remove` is how a client is *supposed* to learn, and it needs a round trip.

Neither saved `kitty`, which exits on an output replacement for reasons of its own
(`ConnectionClosed`, no stderr). Chrome survives. See **I16** — the reconfigure path is not
broken in general.
