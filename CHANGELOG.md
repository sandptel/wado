# Changelog

## Unreleased

### Added

**Every aspect ratio, and a straight answer about each one.** The resolution picker now has
two groups. *Recommended* is derived from your own screen — five rungs from 540p to 1440p, all
of them exactly your panel's shape. Below it is every standard mode there is: 16:9, 16:10, 3:2,
4:3, 5:4, 21:9, 32:9, 20:9, 19.5:9, 18:9 and square, in your device's orientation.

The point is the labels. A mode that is not your screen's shape is drawn with black bars, and
**a bar is not part of the session** — a tap there reaches nothing, which is what "I cannot
click everywhere" meant. So every option says which it is: *"1920 × 1080 — 16:9, bars at the
sides, 20% of the screen unused"* against *"2400 × 1080 — 20:9, fills this screen"*. The
sentence under the picker repeats it for whatever is currently chosen.

**A session orientation setting, and a fullscreen lock that lets go.** Entering fullscreen pins
a phone to the session's orientation, and the release was written only into the ⛶ button's own
exit path — so leaving fullscreen by the gesture, the system back button or Escape left the
page pinned to landscape with nothing on screen able to release it. The unlock now happens on
`fullscreenchange`, which covers every way fullscreen can end.

Alongside it, *Orientation* in session settings: **Auto** keeps the existing rule (landscape on
a phone, as-is on a desktop — a phone at rest is held upright, and an unthinking start gave
desktop applications a 360px-wide screen), **Landscape** and **Portrait** force it either way
on any device. It decides the shape of the resolution options too, so the two stay consistent.

**Lay the gamepad out yourself.** Gamepad settings gained an *Edit layout* switch: with it on,
every control on the pad drags where you want it, and − / + resize whatever you last touched.
The default layout is a guess about a hand it has never seen, and a thumb is not in the same
place on a 6" phone as on a tablet.

A control moves with the cluster it belongs to, not alone: the D-pad is four arms of one pad and
drags as one piece, and resizing it spreads the arms apart rather than growing them into each
other. Same for each stick, the face buttons, each shoulder pair and select/home/start.

Positions are stored as a fraction of the stage rather than in pixels, so the same layout lands
under the same thumb in a session of a different resolution — and the layout lives in the
browser, beside the controls it describes, so it survives a reload. Nothing presses while edit
mode is on, ⟲ puts a cluster back, and *Reset the layout* in settings puts them all back.


**An on-screen gamepad.** New 🎮 button on the bar puts a translucent controller over the
video — two sticks, a D-pad, four face buttons, four shoulders, select/home/start — and it
works in one of two ways, which is a real choice rather than a preference.

The second mode is the one worth explaining. Wayland has no gamepad protocol and never has:
games read controllers straight from `/dev/input` through evdev, below the display server
entirely. So "real controller" mode does the only thing that can work — the daemon opens
`/dev/uinput` and creates a virtual Xbox 360 pad on the host, reporting the exact vendor,
product and axis layout the `xpad` driver does, so SDL's built-in mapping database recognises
it and no mapping string has to be pasted anywhere. Steam, Proton and anything else on that
machine see a controller plugged in. **That device is global to the machine** — a kernel input
device is visible to everything that can read `/dev/input`, inside the session and outside it,
and there is no way to scope one to a Wayland session. It is created on the first press and
destroyed with the session, so a session that never opens the pad leaves nothing behind.

The default is the other mode, which maps the pad onto the keyboard and mouse events the
session already carries: left stick walks WASD, **right stick is mouse-look**, A jumps,
shoulders click the mouse, Select is Tab and Start is Escape. Nothing is needed on the host for
this one — and it is also the only mode that can aim, which is what a right stick is for. It is
the default for a blunt reason: `/dev/uinput` is `root:uinput 0660`, and until the daemon's
user is in that group the other mode is dead. When it is, the log says so by name rather than
by errno.

Size, opacity and distance from the screen edges are sliders in a new Gamepad settings group,
applied live.

**Applications can go fullscreen.** `xdg_toplevel.set_fullscreen` was smithay's empty default,
so a game or a video player asking to fill the screen — the first thing most of them do on
startup — got no answer and kept its windowed size. It is granted now: the window takes the
output, the focus ring is suppressed over it (a border drawn over something that owns the
screen is a border drawn into the picture), and a rotation or a scale change re-sizes it
instead of leaving it at the old output's dimensions. Restoring puts it back where it was, even
if it was maximized first.

### Changed

**The gamepad tells you it was pressed.** A pressed control now grows and throws a ring wider
than the fingertip covering it — the old feedback was a colour change under the thumb that
pressed it. Done with the `scale` property rather than a `transform`, so it survives a control
that has been dragged somewhere else.


**A phone session is now always landscape.** The browser reports the panel in whichever
orientation the phone is held, and a phone at rest is held upright — so a session started
without thinking about it came out 720×1600, which is a 360-pixel-wide screen for desktop
applications that mostly refuse to be that narrow. The output size is fixed at Start and cannot
be changed afterwards, so this is the only moment the choice exists. On a phone the long edge
is now the width; fullscreen then locks the handset to landscape, because that already follows
the session's own shape. Tablets and desktops are untouched.

**The four window actions moved behind one ⊟ button on the bar.** Eleven buttons in a row is
more than fits across a phone held in landscape, which is now the orientation every phone
session is in. They pop up as a row above the bar and close after acting — the popup covers the
video it is acting on, and "next window" is the one action you cannot judge without seeing the
result. Nothing was removed.

**Pointer lock, for 3D games.** New 🎯 button on the bar: it takes the mouse, and the mouse
stays taken until you press Escape. Two halves, and neither works without the other. In the
browser it is Pointer Lock, which stops reporting a *position* and starts reporting *movement* —
the fix for a first-person camera that stops turning the moment the pointer reaches the edge of
the video while the real mouse keeps going, and then leaves the page entirely. In the compositor
it is `zwp_relative_pointer_v1` and `zwp_pointer_constraints_v1`, advertised for the first time;
SDL, GLFW and the engines on top of them look for **both** before they will enable relative mouse
mode, and fall back to warping the pointer back to the centre when either is missing. Verified by
reading the registry a real client is offered.

Deltas are summed rather than dropped when they are rate-limited to the display's refresh — a
1000 Hz mouse coalesced newest-wins would have thrown away fifteen sixteenths of every flick.
Clicks and scrolls that arrive during a lock act on the pointer's real location, not on the
frozen one the browser reports. Escape is the release, and it is the browser's guarantee rather
than wado's, so there is no way to be stranded; the button follows `pointerlockchange`, so it
also turns itself off when a tab switch or leaving fullscreen drops the lock. Not available on
iPhone Safari, which says so rather than doing nothing.

**A glow around the focused window.** Two rings of solid colour tiled around the focused
window's edge — a 2px accent border and a wider one at low alpha for the falloff. It exists
because focus here is not what focus is on a desktop: every window action on the bar acts on
"the focused window", with focus-follows-pointer it moves silently under the pointer, and the
usual answer — the application's own title bar — is not available to a server-side-decorated
window, to an X11 client under the session's Xwayland, or to anything fullscreen.

Drawn from buffers held on the session rather than rebuilt per frame, which is the whole
performance story: a stable element id and commit counter mean a focused window sitting still
adds nothing to the frame's damage. Measured with a static window — 1 damage rectangle per
frame with the ring on screen, 1 without. Rebuilding the elements each tick would have made
every frame a full-screen repaint.

### Fixed

**Both analog sticks can move at once.** In "real controller" mode the two sticks shared one
slot in the input coalescer, so a sample from one could replace a sample from the other and
the losing stick simply stopped. Each axis now has its own slot, and a sample that repeats the
value already sent is dropped rather than queued — a thumb held at full deflection was
restating the same number sixty times a second.


**Windows no longer open, or stay, bigger than the screen.** Three gaps that failed together.
`xdg_toplevel.configure_bounds` was never sent, so a client had no idea how big the screen was
and opened at whatever it uses on a desktop; the initial configure carried no size, so nothing
followed up; and the reconfigure path resized only *maximized* windows, so raising the scale
shrank the logical output underneath every window already open and told none of them. Measured
on a 640x360 logical screen — 720p at scale 2, the phone case — kitty opened at 884x1078 and now
takes 640x360; nautilus opened at 890x550 and now takes 640x380. Raising the scale under an
already-open nautilus used to leave it at 890x550 and now shrinks it the same way.

The two numbers that are not the screen size are the honest part: nautilus will not go below 380
high and gnome-calculator will not go below 616, and a client is allowed to refuse a configure it
cannot honour. Scale is a divisor on the logical output, so at scale 2 a 720p stream is smaller
than the minimum size of a good many desktop apps. Nothing at the protocol level fixes that —
scaling the window at render time does, and that is its own piece of work.

Restoring a maximized window now re-fits the size it was remembered at, which could have been
measured against a larger output before a reconfigure.

### Added

**The drawer shows everything, behind one eye.** Three changes, one complaint: applications
were missing from it. The grid was capped at 40 tiles, which on this machine hid 32 of the 72
it had found — the cap is now a DOM guard at 400 rather than a curation. The server had also
been *dropping* every entry marked `NoDisplay` or `Hidden`, 71 of the 143 installed here; it
now carries them with a flag, because "not a menu item" and "not launchable" are different
claims and only the first one is true. Those, and the entries no installed theme has an icon
for, sit behind 👁 on the search row — one tap to list everything and search it, and the
button says how many more that is. Beside it, ▶ runs what is in the box, as Enter now does:
text naming an installed application by name or by command launches that application, anything
else is run verbatim with its flags. The whole list costs 869 KB per open with the hidden half
included, up from 645 KB — the icons, as before.

**An X server for the session, for apps that cannot speak Wayland.** New setting, off by
default: the session runs its own rootful Xwayland and every application launched into it gets
that `DISPLAY`. Steam is the case that prompted it — it is X11-only, so with the isolation
above it could only fail, and before that it opened on the host's desktop. Verified by running
it: the store rendered in the stream, from a frame pulled out of the encoded output. Rootful
rather than rootless because rootless needs the compositor to be the X window manager, which is
a milestone and not a patch — so the cost is named where the setting is: every X application
shares one screen, that screen is a single window the size of the stream, it is there even when
nothing is using it, and nothing inside it manages windows.

**An app drawer, on the bar.** ⊞ opens a sheet over the video: a search box that is also the
command line, a row of what you launched recently, and a grid of every installed application
with its icon. Tap launches and closes it; long-press (right-click on a desktop) drops the
command in the box instead, for when you want to add a flag first. Launching used to cost
open-settings → scroll past the session group → type → Launch, which is four deliberate actions
for the thing people do most on a phone. Sizes are fluid rather than stepped — one `clamp`
covers a 360px phone, the same phone in landscape, a tablet and a docked desktop panel better
than three breakpoints did.

**Icons in the list.** The server resolves each desktop entry's `Icon=` against the icon themes
installed on the machine and carries the file inline as a `data:` URI, because the client is a
web page that cannot read the server's filesystem and, in relay mode, has no HTTP route back to
it at all. One cached directory walk, the closest size bucket to 64 px, 32 KiB per file — 72
applications came to 661 KB here. An application whose icon is not installed gets a letter tile.

**A dot on what is already running.** The list says which of its entries the session has a live
process for, joined on the exact command that launched it, refreshed each time the drawer opens.
It means the process is alive rather than that a window is mapped: the alternative is matching
`xdg_toplevel.app_id` against a desktop entry's `Exec`, and those two strings disagree constantly
(`org.gnome.Nautilus` versus `nautilus`).

**Session applications no longer open on the computer running wado.** New setting, on by default:
the session gets its own D-Bus bus and its applications are launched with no `DISPLAY`. Both
halves are needed and neither is obvious. A single-instance application — a browser, a file
manager, most GTK apps — asks the *session bus* whether a copy of itself is running, finds the
one on the host desktop, and hands its command line to it; the window then opens over there and
launching appears to have done nothing. And wado has no Xwayland, so an X11 client cannot draw
here at all — with `DISPLAY` inherited it draws on the host's X server, which Chromium and
Electron make the common case rather than an edge one by preferring X11 whenever `DISPLAY` is
set, `WAYLAND_DISPLAY` or not. The cost is named in the UI: the private bus is empty, so
notifications and file-chooser portals are gone, and an X11-only application now fails where you
can see it. Audio is untouched — PipeWire and PulseAudio are reached through `XDG_RUNTIME_DIR`.

**A run has a lane now, and the log says which.** `WADO_RUN=perf|connection|feature|compositor`
picks what this session is investigating; each lane is an `EnvFilter` string and nothing more, so
switching costs a daemon restart rather than a rebuild. Deliberately not a Cargo feature: a lane
changes several times an hour, and a feature would mean a full LTO release build of the one binary
that must never be a debug build, to change a log level.

**Every WebRTC line names the device that caused it.** Offers, answers, ICE states and the connect
now carry `peer=<addr> room=<id>`. With a pool of daemons and several phones, a log without this
cannot be read at all — the monitor had been using the offer's *candidate count* as a device
fingerprint, and on `2026-09-14` its absence cost three wrong hypotheses about which daemon was
broken.

**One line says why ICE failed.** On `Failed`/`Disconnected`: both sides' candidate types and how
long it spent trying, in a single warning, instead of four lines correlated across two logs.

**wado warns when it is behind a symmetric NAT.** Two STUN servers, one socket, at startup: if
they report different mappings, the srflx candidate in every answer names a port no peer can
reach, and connections hang in ICE `checking` with nothing else logged. A VPN on the default route
does this. Worth the probe because that state had been diagnosed three times as CGNAT and as
access-point isolation, neither of which had ever been measured.

**TURN, when you have a server for it.** `WADO_TURN_URL` (plus `WADO_TURN_USER` /
`WADO_TURN_PASS`) adds a TURN server to the ICE configuration; comma-separated URLs are split,
and a URL that is not `turn:`/`turns:` is refused loudly rather than passed to webrtc-rs, because
a rejected entry is indistinguishable from no TURN at all — ICE simply never produces a `relay`
candidate. Without it, two peers that are both behind a VPN cannot connect, which is not a
hypothetical: it cost twelve minutes between two machines on one WiFi on `2026-09-19`. The
startup NAT warning now says whether TURN is configured, because symmetric NAT with TURN is
survivable and symmetric NAT without it is not.

### Fixed

**The rig stops invalidating the URL devices are holding.** `scripts/rig.sh` reused to kill a
working cloudflared and mint a new quick-tunnel hostname on every start, stranding every device
still pointed at the old one — a failure with *no trace anywhere*, because the request never
reaches the relay. The tunnel is now reused unless it is genuinely dead, and the script warns when
the deployed client's compiled-in default is not the live tunnel.

**`scripts/rig.sh --add N`** grows a running pool without interrupting a session. The moment more
daemons are needed is exactly when devices are connected and one is being refused.

## v0.0.3 — `2026-09-12`

Four Wayland protocols, the scroll fix, and the first thing that responds to a link it cannot
keep up with.

### Added

**Four Wayland protocols apps expect a compositor to speak.**

*Launched apps come to the front.* An app started from the picker or the shell used to draw its
first window behind whatever already had focus — on a phone, a strip of window you then had to
find and tap. Toolkits have always passed a token through for this; wado now listens
(`xdg_activation_v1`). Tokens work once.

*Animations can pace themselves.* Apps can now ask when a frame was actually shown
(`wp_presentation`), which is what GTK and Chrome use to keep an animation smooth instead of
guessing. wado answers with the instant compositing finished, and explicitly does **not** claim
the hardware guarantees a real display would provide — a confident wrong answer here is worse for
an app than no answer, which is why this one waited. Confirmed live with two independent
toolkits, Chrome and GTK4.

*Solid backdrops stop costing a full frame.* The grey sheet behind a dialog is one colour, and a
toolkit can now say so in a single pixel rather than allocating and uploading a screen-sized image
every time it changes (`wp_single_pixel_buffer_v1`).

*Apps can declare what they are drawing* — video, a game, a photo (`wp_content_type_v1`). Nothing
acts on the hint yet, and it is in the log for a reason: whether the encoder should treat video
differently is worth answering only once we know real apps bother to say. So far none have.

Two protocols stay unimplemented on purpose. Cursor shapes are meaningless in a session that
draws no cursor. Frame-pacing (`wp_fifo_v1`, `wp_commit_timing_v1`) needs the render loop to be
able to hold a finished surface back until it is due, which it currently cannot — and advertising
the promise without keeping it would make pacing worse, not better.

**Reconnecting offers a choice instead of a dead end.**

A session that was still running used to answer a returning viewer with "a session is already
active" and hang up — leaving the session alive, visible in the logs, and unreachable. Now you are
told what is running and asked: **rejoin it**, keeping its windows and applications exactly as you
left them, or **drop it** and start fresh with this device's settings.

There is deliberately no default. Rejoining silently would ignore a resolution or scale you just
changed, because a session's output cannot be resized once open; dropping silently would kill
someone's work because their tunnel hiccuped.

**Clients can hand over GPU buffers** — `zwp_linux_dmabuf_v1`.

`wl_shm` was the only buffer path on offer, so a GPU application had to render on the GPU,
read the result back to the CPU, write it into shared memory, and have the compositor upload
it to a texture again — two full copies of every window, every frame, on the render tick. At
a phone's 1080 × 2422 that is roughly 10 MB per surface per frame. Version 4 with feedback is
advertised when the render node is known, so a client is also told *which* GPU to allocate on;
version 3 otherwise.

**Two-finger pinch and rotate** — `zwp_pointer_gestures_v1`.

A two-finger drag now produces a scroll axis *and* a pinch. That is what a touchpad emits and
what toolkits are written against — so the pinch's own translation is deliberately sent as
zero, or an app pans twice for one drag.

**Windows are borderless** — `zxdg_decoration_v1`, answered server-side.

Apps stop drawing their own titlebar, shadow and frame. On a phone that strip cost scarce
vertical space and its buttons were too small to hit, while maximize, minimize, close and cycle
already arrive from the control bar — and dragging a window never needed a titlebar, because
long-press-drag moves it. GTK keeps its header bar, which is application content rather than
decoration.

**A keyboard button, for devices that have no keys.**

Tapping ⌨ raises the phone's soft keyboard. The Wayland answer for this is inert in wado — every
text-input request is dropped unless an input-method client is bound — so the button focuses a
hidden field instead and translates what you type into key events. Android does not report key
codes for its soft keyboard, so characters are read from the text itself; Enter, Backspace and
the arrows still come through as keys. US layout for now.

**The fps picker knows what your screen can do.**

No browser API exposes a refresh rate, so the page times its own frames and reports the median.
The rate now appears under the picker, with a warning when the rung you chose is above it —
because those extra frames are never shown, and every frame that *is* shown gets fewer bits for
them.

**A resync button.**

Rebuilds the video connection while leaving the session, its windows and the shell running. This
is the only thing that clears the delay a network hitch permanently adds to playback: that buffer
belongs to the browser's receiver, and a receiver is created fresh with each connection. Manual
on purpose — doing it automatically would fire hardest on exactly the bad links where dropping a
connection helps least.

**A one-command test rig.** `scripts/rig.sh` starts the relay, the tunnel and the daemon, and
prints the relay address and Remote ID you need to connect. `--daemon` restarts just the daemon
after a rebuild, which matters because restarting the tunnel changes its address and invalidates
whatever your phone is pointed at.

### Changed

**The compositor stops making frames the network cannot take.**

Measured on a tethered mobile link: 420 encoded frames discarded in a single session — captured,
composited, encoded, and thrown away, six seconds of video that cost a GPU readback each and was
never going to be seen. The render loop now watches how many frames the sender is refusing and
renders less often when that number moves, easing back only after the link has been clear for a
while. Backing off quickly and recovering slowly is deliberate: a link this variable will
oscillate under symmetric control, and flicking between smooth and stuttering looks worse than a
steady lower rate.

**This is not bandwidth estimation and does not adjust quality.** The encoder's bitrate is fixed
when a session opens and neither backend can change it on the fly, so the only lever the render
loop holds is how often it produces a frame at all. What it buys is that congestion now costs
frame rate instead of costing work — the frames it skips are exactly the ones that were being
discarded anyway. Real link measurement remains the open problem.

**Logs answer two questions they used to leave open.**

Every session start now records **bits per pixel** — the single number that predicts whether a
resolution, frame rate and bitrate can look good together. Frame rate and bitrate are separate
settings, so moving 60 → 120 quietly halves what each frame gets; that is now visible before
anyone squints at the picture.

The video pump used to log only stalls past 100 ms, which hides the shape of everything else: a
pipeline that is fast with rare spikes and one that is slow all the time produce identical
warnings. It now reports the full distribution once per few seconds, and still calls out
individual stalls.

**Logs say where things went wrong instead of going quiet.**

One `EnvFilter` sat on the subscriber registry, so it gated every destination at once — the
terminal and the client's log panel shared a single verbosity, and because that verbosity was
`info`, every `debug!` in wado's own crates was written, shipped and permanently silent. A log
line that *cannot* fire is indistinguishable from one whose subject never happened, which is
the worse of the two failures.

Filters are per-destination now. The terminal defaults to debug for wado's own crates, with
`webrtc_ice` muted — one session teardown emitted eight "Failed to close candidate" lines,
none of which ever meant anything. The client's log panel stays at `info`, because it is a
200-line ring in front of a person. `RUST_LOG` still overrides the terminal. Per-lane detail
needs no new code: a tracing target is the module path, so
`RUST_LOG=wado_compositor::headless=debug` already works.

Two questions that previously required catching a live session and reading `/proc` now answer
themselves in the log: whether any client took the dmabuf path (said on **both** branches —
"live" or "unused this session"), and which surfaces asked for a fractional scale, which is the
only way to see the population that the integer fallback actually serves.

**TURN, when you have a server for it.** `WADO_TURN_URL` (plus `WADO_TURN_USER` /
`WADO_TURN_PASS`) adds a TURN server to the ICE configuration; comma-separated URLs are split,
and a URL that is not `turn:`/`turns:` is refused loudly rather than passed to webrtc-rs, because
a rejected entry is indistinguishable from no TURN at all — ICE simply never produces a `relay`
candidate. Without it, two peers that are both behind a VPN cannot connect, which is not a
hypothetical: it cost twelve minutes between two machines on one WiFi on `2026-09-19`. The
startup NAT warning now says whether TURN is configured, because symmetric NAT with TURN is
survivable and symmetric NAT without it is not.

### Fixed

**Scrolling asked for about 1.6x too much finger.**

Touch and wheel deltas were measured in the *viewer's* pixels and spent in the *session's*, with
nothing converting between them — so dragging a page moved it roughly two-thirds as far as your
finger went. The speed slider could not fix this, because the right number depends on your screen
size, the session resolution and its scale, and changes whenever any of them does.

Both scroll paths now convert properly, which gives the thing a touchscreen is supposed to do:
content moves exactly as far as your finger does, measured on the glass. The same bug was making a
real mouse wheel under-scroll on desktop viewers, and that is fixed with it.

**Fractional scale was implemented and then rounded away.**

The requested scale was rounded to a whole number *before* it reached either consumer, so a
session asking for 1.25 was given 1.0 on a protocol wado already speaks. The output now
carries both answers: the exact value for clients that speak `wp-fractional-scale-v1`, a whole
number for those that do not. Verified live at 1.25, 1.75 and 2.0.

The whole-number companion is floored, not rounded. Rounding and ceiling agree at 1.5, 1.75,
2.5 and 2.75, so rounding would have left the original bug — an oversized buffer overhanging
its own area and clipping app chrome — intact at every scale above 1.25. Flooring makes a
legacy client slightly soft instead, which costs almost nothing through an H.264 stream.

**A pinch that was never ended left the toolkit stuck in zoom mode.**

A disconnect or an input reset drops the gesture with no end event, and windows outlive
sessions here, so the orphan survived into the next one. The open/closed state is tracked on
the compositor side now; a new pinch closes any open one first.

**A client could ask for an output large enough to take the daemon down.** Session dimensions
arrived from the network unbounded, so a request for 100000 x 100000 was a forty-gigabyte
allocation attempt. Clamped to a supported range, and the clamp is logged.

### Removed

**The old one-shot command runner.** The real shell replaced it; nothing could reach it any
more. 272 lines across four crates, and with them a way for a process to outlive the session
that started it.

### Retracted

**Jitter-buffer reclaim (shipped in v0.0.2) never worked, and is deleted.**

`jitterBufferTarget` is a floor honoured only up to what the browser's own timing model
demands — it can raise the playout delay and can never lower one the model is driving. The
reassert fired 15 times in one session while the buffer drained 56 → 55 → 54 → 54 → 53 → 52,
its natural rate, with no inflection at any of them. The reasoning is left in `webrtc.js` so
it is not tried again. The underlying behaviour is still real and still unfixed: a network
hitch permanently inflates the buffer by about 5 ms.

## v0.0.2 — `2026-09-11`

Everything v0.0.1 measured still holds; this adds the fixes found by running it. Each item
below says how far it was actually verified, because two of them have never been watched by
a human.

### Verified live

- **Launched applications no longer outlive their session.** An app spawned into the session
  inherited the daemon's own process group, so stopping the compositor killed the one pid we
  held and nothing it had forked — a browser kept running, and kept playing audio, with
  nothing left that knew how to stop it. Apps are now spawned as their own process-group
  leaders and signalled by group (SIGTERM, 300 ms grace, then SIGKILL). A signalfd handler
  runs the same teardown on SIGINT/SIGTERM to the daemon itself.
  Confirmed on the running daemon: `signal received — stopping session` → `compositor
  session stopped` → zero survivors.
- **The 1080p120 flashing is gone.** The VBV was sized in *frame times*, which made the
  allowance frame-rate dependent: at 120 fps it halved, and IDRs were being squeezed to
  ~16 KB. Sized in bits now (`VBV_MILLIS = 67`). Keyframes came back at 45–52 KB with 0
  pump stalls.

### Measured, on a release build

- **Quality presets scale with resolution.** `Balanced` and `Optimise quality` were handing
  1080p the same bitrate as 720p. Bitrate is now scaled by pixel count from a 720p base and
  clamped to 1–12 Mbps; `Custom` still passes through untouched.
- **The 1080p stutter was a debug build**, not the pipeline. Same config, release vs debug:
  stalls 14 → 0, jitter buffer 19–42 ms → 12 ms flat, frame rate 48–60 → 60 steady. The
  server now warns on startup when it is a debug build.

### Shipped, not yet confirmed by a human

- **A real shell in the console, on a PTY** (`$SHELL -l`) with xterm.js in front of it —
  resize, Ctrl-C and full-screen programs all work by construction rather than by
  observation. Server and client are deployed and byte-verified; nobody has watched it run.
  It does **not** survive a reconnect yet: the shell is owned by the relay connection, so
  losing it starts a fresh one.
- ~~**Jitter-buffer reclaim after a network blip.**~~ **Retracted** — it never worked; see
  the Unreleased section above.

### Packaging

- **`flake.nix` and `flake.lock` are in the repo**, with a real `packages.default` — so this
  tag can be `nix build`-ed. v0.0.1 predates the flake and cannot be; it is left where it is
  rather than moved, since it is already published.

### Known ceilings

- An app that calls `setsid` escapes the process-group cleanup (seen: `chrome_crashpad`).
  A per-session cgroup is the upgrade path.
- Pump stalls of 300–500 ms still appear occasionally on LAN with `rtt=0` and no packet
  loss. Duration does not track frame size or packet count, so it is a blocking event rather
  than send cost — cause not yet established, and deliberately not guessed at.

## v0.0.1 — `2026-09-11`

The first tagged point. It marks a **measured latency local maximum**: the settings below
were reached by tracing real sessions, and every number here was read off a running system
rather than estimated.

### The measured state

Session: **1728×1080 @ 120 fps**, 8000 kbps, `Veryfast`, keyframe interval 240, scale 1.0,
pipeline `vaapi-dmabuf` (`tier=VaapiDma`, hardware encode), **release build**.

| Reading | Value |
|---|---|
| Client frame rate | 119–120 fps, steady |
| Jitter buffer | **13 ms, flat** (no upward drift) |
| Frames dropped (client) | 0 |
| Packets lost | 0 |
| `write_sample` stalls | **0** |
| Pump queue wait | avg 0 ms, worst 1–7 ms |

**Read this caveat before quoting the numbers.** They were taken at `rtt=0ms` — a local
path, not the internet. They measure everything wado controls (capture, encode, queue,
packetise, decode) and nothing it does not. They are *not* a glass-to-glass figure over
cellular, and the project's ~80–100 ms target is about that harder number.

Also holding at 1728×1080 @ 60 fps on both 4000 kbps (`Balanced`) and 8000 kbps
(`Quality`): zero stalls, jitter buffer flat at 12 ms.

### What made it possible

**Never run a debug build.** The single largest finding of the session. A `./target/debug`
daemon cannot meet the target and does not fail like a build problem — unoptimised SRTP and
per-packet packetisation produced 100–200 ms pump stalls, a jitter buffer climbing 19→42 ms,
oscillating throughput and frame rate dipping to 48 at 1080p. Every one of those symptoms
reads as a network or encoder fault. The same configuration on a release build: zero stalls,
flat 13 ms. A startup `WARN` under `cfg!(debug_assertions)` now says so out loud.

**VBV sized for latency, not streaming.** `rc_buffer_size` was one full second of bitrate,
which let keyframes reach 250–350 KB and stall the pump for 110–153 ms. It is now four frame
times.

**A playout-delay hint** to the browser, and an ICE gathering cap so a STUN server that never
answers cannot hold the offer back.

**TURN, when you have a server for it.** `WADO_TURN_URL` (plus `WADO_TURN_USER` /
`WADO_TURN_PASS`) adds a TURN server to the ICE configuration; comma-separated URLs are split,
and a URL that is not `turn:`/`turns:` is refused loudly rather than passed to webrtc-rs, because
a rejected entry is indistinguishable from no TURN at all — ICE simply never produces a `relay`
candidate. Without it, two peers that are both behind a VPN cannot connect, which is not a
hypothetical: it cost twelve minutes between two machines on one WiFi on `2026-09-19`. The
startup NAT warning now says whether TURN is configured, because symmetric NAT with TURN is
survivable and symmetric NAT without it is not.

### Fixed in this release

- **Launched applications no longer outlive their session.** They inherited the daemon's own
  process group, so `child.kill()` reaped one pid and left the rest — a browser kept playing
  audio after the session that owned it was gone. Applications now get their own process
  group and a group-wide SIGTERM → 300 ms → SIGKILL.
- **The daemon shuts down on a signal.** There was no handler anywhere in the tree, so
  SIGINT/SIGTERM killed it outright and `stop_session` never ran. A calloop signalfd source
  now stops the session first.
- Session leak on relay disconnect ("a session is already active").
- `wp-fractional-scale-v1` and `wp-viewporter`, so a client told 1.5× draws at 1.5× instead
  of drawing at 2× and being composited clipped.
- Device-exact resolutions derived from the device's own aspect ratio, and a Hyprland-style
  application scale control.
- Two-finger scrolling with `AxisSource::Finger` semantics and a terminating axis-stop, so
  kinetic scrolling ends when the finger lifts.
- A console (shell + log) that floats over the picture instead of stealing height from it.

### Known limitations

- **Quality presets use a flat bitrate that ignores resolution.** `Balanced` asks for 4000
  kbps at 720p and at 1080p alike — roughly 2× starved at 1080p. Unfixed deliberately:
  webrtc-rs 0.17 has no congestion control in its send path, so an uncapped increase would
  fall apart on cellular while looking fine on a LAN.
- **The relay is not safe to deploy publicly.** The ~30-bit Remote ID is currently the only
  secret. Join rate-limiting, a confirmation gate and TLS are all required first.
- **The control plane binds localhost** because the launch command is free-form.
- Console-shell processes are still not tied to session lifetime (`server/src/exec.rs`).
- An application that calls `setsid` for itself escapes the process-group cleanup —
  observed with `chrome_crashpad`.
- No PTY: the console runs commands non-interactively, so an editor or pager will not work.

### Not included

No binaries. The daemon links the Nix dev shell's FFmpeg 8.x ABI and VA-API stack, so a
binary built here would not run elsewhere — publishing one would be worse than publishing
none. Build from source in the dev shell.
