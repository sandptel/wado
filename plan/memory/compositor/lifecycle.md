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

## Launched apps are isolated from the host desktop (since 2026-09-19)

**The complaint:** "I launch it in wado and it opens on my computer's desktop." Setting
`WAYLAND_DISPLAY` — which `build` has always done — does not prevent it, for two reasons:

1. **The session bus.** A single-instance app (browser, file manager, most GTK apps) asks the
   *session bus* whether a copy of itself is running. On a dev desktop one is, so the new
   process hands over its command line and exits — and the window opens over there.
2. **`DISPLAY`.** wado has **no Xwayland at all**, so an X11 client cannot draw here. With
   `DISPLAY` inherited it draws on the host's X server. Chromium and Electron make this the
   common case, not an edge one: they pick X11 whenever `DISPLAY` is set, *even with*
   `WAYLAND_DISPLAY` present.

**The fix** (`compositor/src/session_env/`): `SessionConfig.isolate_apps` (default **on**,
locked while a session runs). At Start the session spawns its own `dbus-daemon --session
--print-address --nofork`, and every launched app gets `DBUS_SESSION_BUS_ADDRESS` pointing at
it with `DISPLAY` removed. The bus is killed after the apps, on stop.

**Cost, accepted:** the private bus is empty — no notification daemon, no XDG portals, no
secrets service. Audio is unaffected (PipeWire/Pulse are `XDG_RUNTIME_DIR` sockets, left
alone). X11-only apps now fail visibly instead of opening on the host.

**Not re-applied on reconfigure**, deliberately: apps already running hold the old address, and
a session split across two buses is worse than either answer.

⚠️ **`systemd-run` expands `${VAR}` in the command line before the shell sees it.** A test
probing the child's env with `printf "${DISPLAY-unset}"` read blank no matter what the
environment held — it fails as "the isolation does not work" when the isolation is fine. Dump
`env` to a file and assert in Rust instead.

## X11 apps run inside the session, rootfully (since 2026-09-19)

Isolation made X11-only apps fail honestly instead of opening on the host desktop. Steam is
X11-only, so "honestly" meant "not at all" — hence `SessionConfig.x_server` (off by default):
`Xwayland :N -geometry WxH -noreset` as an **ordinary Wayland client of the session**, with
`DISPLAY=:N` for everything launched. `compositor/src/session_env/xwayland.rs`.

**Verified, not reasoned** (2026-09-19): Steam's store rendered in the stream, logged in, read
off a frame extracted from the session's own H.264 output with ffmpeg. End-to-end through
`handle_command(Start)` afterwards: `app_x = :20`, launched app sees `DISPLAY=:20` plus the
private bus, socket cleaned up on stop.

**Rootless is the upgrade path and is a milestone.** It needs the compositor to implement the
XWM side (`-wm`); without it X windows are never mapped as Wayland surfaces at all. Rootful
needs nothing from the compositor, which is why it shipped first. Its ceiling: one shared X
screen, sized at Start, no window manager inside it.

### Traps found while building it

- **`Xwayland -displayfd` reported a number that was already in use** on this machine (`0`,
  while the host session held `:0`). Scanning `/tmp/.X11-unix/XN` *and* `/tmp/.XN-lock` from
  `:20` up, then waiting for the socket to appear, is what replaced it — the socket existing is
  a state, a printed number is not.
- **`xterm` is a broken test client here.** It dies with `fatal IO error 11` against wado's
  Xwayland *and* against the host's X server, and its core bitmap font is missing too. Three
  probes were nearly misread as "the X server does not work". `xsetroot -solid red` returns 0
  and is a better liveness check; Steam itself is the real one.
- **An example that builds `Wado` by hand must set `WAYLAND_DISPLAY` itself.** `build()` does
  it; `Wado::new` + `init_headless` does not, so a probe's Xwayland silently connected to the
  *host* compositor and mapped nothing here. The giveaway was globals in its registry that wado
  does not implement (`xdg_system_bell_v1`, `zxdg_exporter_v2`).


## Windows, the output, and what a client may refuse (2026-09-19)

**`configure_bounds` was never sent.** A client that is not told how big the screen is opens at
its desktop default. GTK4/Qt6/recent Electron honour the hint; `crates/compositor/src/fit.rs`
sends it at map time and re-sends it on every reconfigure.

**Scale is a divisor on the logical output.** 720p at scale 2 is a 640×360 logical screen.
`refit_windows` used to resize only *maximized* windows, so raising the scale left every other
window at its old size — that is the "changing the zoom doesn't resize anything" report, and it
was never about zoom.

⚠️ **A client may refuse a configure, and the good ones do.** Measured on 640×360: kitty complies
(884×1078 → 640×360), nautilus stops at its 380px minimum height (890×550 → 640×380),
gnome-calculator refuses entirely (616 high). No protocol-level fix exists for this. Render-time
rescale is the only answer, and its hard half is the inverse transform for input.

⚠️ **kitty exits when a session is reconfigured.** Measured 2026-09-19, and it happens on an
unmodified tree too — not caused by the fit work. GTK apps survive the same reconfigure. Suspect
`retire_output_global`. This contradicts the documented promise that reconfigure keeps
applications alive, so do not repeat that promise without qualifying it.

## Focus glow: damage is the whole design (2026-09-19)

`crates/compositor/src/glow.rs` draws the focused window's ring as custom elements passed to
`render_output`. **The `SolidColorBuffer`s live on `Wado`.** Built fresh per frame they would
carry a new `Id` each tick, which the damage tracker can only read as a full-screen repaint —
forever, at a fixed bitrate. Measured with a static window: 1 damage rect/frame with the ring on
screen, 1 without.

Custom elements render **in front of** the space, so a ring cannot be an expanded filled rect
behind the window. It is four non-overlapping quads tiled around the geometry; non-overlapping
matters because the outer ring is translucent and two stacked quads show a bright seam.

## Pointer lock needs both protocols (2026-09-19)

`zwp_relative_pointer_v1` alone is not enough: SDL/GLFW check for `zwp_pointer_constraints_v1`
too and fall back to warping when it is missing. Both are advertised unconditionally in
`state.rs::new` — verified by reading the registry `WAYLAND_DEBUG=1` shows a real client.

The browser half is the security boundary: Escape releases the lock and cannot be intercepted,
so there is no way for wado to strand a pointer. `pointerlockchange` is the only source of truth
for the button's state.

⚠️ **While locked the browser freezes `clientX`/`clientY`.** A click or scroll arriving during a
lock carries a pre-lock position; acting on it teleports the pointer. `locked_pointer_location`
substitutes the pointer's real location and suppresses the motion.
