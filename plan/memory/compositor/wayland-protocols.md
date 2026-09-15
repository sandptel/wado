# wayland protocols — what wado speaks, what it does not

Reference: <https://wayland.app/protocols/>. Smithay module names below are the ones present
in the pinned revision (`~/.cargo/git/checkouts/smithay-*/src/wayland/`) — every "missing"
entry here has a smithay module already, so none of them start from raw protocol XML.

**Touch input itself is complete.** `wl_touch` is advertised (`seat.add_touch()`), and
`input/touch.rs` synthesizes down/motion/up with per-contact slots and a `frame()` after each,
plus `cancel` when a gesture takes over. The phone gaps below are **gestures, on-screen
keyboard and scale** — not the touch foundation.

## Implemented (15 globals)

| interface | via | notes |
|---|---|---|
| `wl_compositor` | `CompositorState` | |
| `wl_shm` | `ShmState` | the CPU buffer path; dmabuf is the fast one |
| `xdg_wm_base` | `XdgShellState` | toplevels + popups |
| `wl_output` / `zxdg_output_v1` | `OutputManagerState` | fresh `Output` per client resolution |
| `wl_seat` (`wl_pointer`, `wl_keyboard`, `wl_touch`) | `SeatState` | all three capabilities added |
| `wl_data_device` | `DataDeviceState` | copy/paste |
| `zwp_linux_dmabuf_v1` | `DmabufState` | **per-session**, not startup — see below |
| `zxdg_decoration_v1` | `XdgDecorationState` | always **ServerSide**, and wado draws none — windows are borderless |
| `wp_fractional_scale_v1` | `FractionalScaleManagerState` | exact; the integer companion is `floor` — see below |
| `wp_viewporter` | `ViewporterState` | companion to fractional scale |
| `zwp_pointer_gestures_v1` | `PointerGesturesState` | pinch driven; swipe/hold advertised only |
| `xdg_activation_v1` | `XdgActivationState` | every token honoured, removed after one use |
| `wp_presentation` | `PresentationState` | `CLOCK_MONOTONIC`, no scanout flags — see below |
| `wp_single_pixel_buffer_v1` | `SinglePixelBufferState` | renderer already handled `BufferType::SinglePixel` |
| `wp_content_type_v1` | `ContentTypeState` | **logged only**, nothing reads the hint |

## ✅ Fractional scale, and why the integer half is `floor`

`headless.rs` used to round the requested scale **before** both consumers, so a client asking
for 1.25 was given 1.0 on a protocol wado already implements (`requested=1.25 applied=1.0`).
`634c603` added that rounding before the protocol existed; `8e9f3e7` implemented the protocol
and re-enabled the fractional UI options on top of the rounding instead of removing it.

Fixed in **8f363e6** with `Scale::Custom { advertised_integer, fractional }` — one answer per
audience. Verified live: `scale=1.25`, `1.75` and `2.0` sessions all start with no `applied=`
divergence.

**The integer half is `floor`, changed from `round` on 2026-09-12.** `round == ceil` at 1.5,
1.75, 2.5 and 2.75, so `round` left the original clipping bug intact at every scale above 1.25,
including the 1.75 in live use. Nothing caught it because **every client tested was Chrome and
Chrome binds `wp_fractional_scale_v1`** — the legacy branch was never exercised, so "no clipping
observed" was zero evidence, not weak evidence. `floor` inverts the failure mode: told 1, a
legacy client draws 1x and is composited soft instead of drawing 2x and overhanging its area.
Through an H.264 stream soft is nearly free; clipped chrome is broken.

`integer_scale()` has **no consumer in `crates/`** — render and capture size from the output
*mode* — so this is the `wl_output.scale` event and nothing else.
`handlers/mod.rs::new_fractional_scale` now logs every surface that takes the fractional path,
so the population the integer branch serves stops being invisible.

## Missing — what is left, and why each one is left

| interface | smithay module | verdict |
|---|---|---|
| `wp_cursor_shape_v1` | `cursor_shape.rs` | **skip permanently.** It exists so a client can name a cursor instead of shipping a surface. wado draws no cursor at all (`SeatHandler::cursor_image` is an empty body and there is no pointer overlay in the render path), so both halves are no-ops here. Implementing it would produce a global whose only effect is to make the compositor ignore a request slightly earlier. |
| `wp_fifo_v1` / `wp_commit_timing_v1` | `fifo/`, `commit_timing/` | **deferred, with a real blocker.** Both are promises about *when* the compositor will latch a commit: fifo says "do not latch this until the previous one was presented", commit-timing says "do not latch before timestamp T". Honouring either means the render tick must be able to *withhold* a surface from the composite it is already building, i.e. a per-surface barrier checked inside `render_output`. Today the tick composites whatever `space` holds at the instant the timer fires and has no concept of a surface that is ready but not yet due. That barrier is the work; the globals are the easy part. Advertising them without it would be worse than absent — an app that asks for fifo and is silently latched early gets *wrong* pacing rather than none, which is the same trap `wp_presentation` had to avoid. |

## ✅ Done since this doc was written

| interface | commit | note |
|---|---|---|
| `wp_fractional_scale_v1` (repaired) | 8f363e6 | was implemented and then rounded away; integer half `round`→`floor` after |
| `zwp_linux_dmabuf_v1` | (this session) | v4 with feedback when the render node is known, v3 otherwise |
| `zxdg_decoration_v1` | f613313 | server-side; see below |
| `xdg_activation_v1` | 4667693 | launcher focus transfer |
| `wp_presentation` | 7f551e2 | software-composite timestamps |
| `wp_single_pixel_buffer_v1` | 7b5c5e3 | one global, no other change |
| `wp_content_type_v1` | 002fc97 | advertised and logged; nothing acts on it |
| `zwp_pointer_gestures_v1` | 198faca, 1dd7c6f | pinch + rotate. Only the **pinch** half is driven — swipe and hold are advertised but never emitted, because the client's two-contact FSM already spends a two-finger drag on the scroll axis and there is no third-contact gesture yet. |

### An orphaned pinch is not the same harmless thing as an orphaned scroll axis

A missing axis-stop costs kinetic scrolling. A missing pinch **end** leaves the toolkit
believing the gesture is still running — it stays in zoom mode, and the next begin lands on an
already-open gesture. Windows outlive sessions here, so an orphan survives into the next one.

Guarded on the **compositor** side (`Wado::pinch_open`), not the client, because the client is
the half that can vanish: a disconnect or `resetInput` nulls `W.gesture` with no end sent. A
begin closes any open gesture first; an update or end with nothing open is dropped.

### Pinch rides the same gesture as scroll, on purpose

One two-finger drag now produces a `wl_pointer` axis **and** a pinch. That is not
double-reporting: it is what libinput emits for a touchpad, and toolkits are written against
it. `GesturePinchUpdateEvent.delta` is therefore sent as **zero** — the translation is already
going out as the scroll axis, and filling in `delta` too makes a toolkit pan twice for one drag.

`scale` is absolute against the begin distance, `rotation` is the delta since the previous
event. The protocol defines them differently; swapping them silently inverts a zoom.

## ⛔ `zwp_text_input_v3` is BLOCKED — it is inert without `zwp_input_method_v2`

Established from the pinned smithay rev (85f83ab), `wayland/text_input/text_input_handle.rs:209`:

```rust
// Discard requests without any active input method instance.
if !self.input_method_handle.has_instance() { return; }
```

**Every** text-input request — `enable` included — is dropped unless an input-method-v2 client
is *bound*. So advertising `zwp_text_input_v3` on its own produces a global that apps bind, use,
and get nothing from. It is not a small change that was skipped; it is a dead global.

Making it work needs `InputMethodManagerState` **and** something bound to it as the IME. wado
would have to be its own input-method client, which means a second Wayland client connection
from inside the compositor process. That is a milestone, not a protocol addition.

**The cheap alternative, and probably the right first move:** the phone OSK problem is 90%
solved client-side by a keyboard-toggle button in the bar that focuses a hidden `<input>`. No
Wayland protocol, no compositor change, and key synthesis over the remote input path already
works. Do that first and measure whether app-initiated OSK is still missed.

`zwp_virtual_keyboard_v1` is separately **not needed**: it exists so a *client* can inject keys
into the seat, and wado already injects them directly via `Wado::key`.

## Missing and deliberately not wanted

`zwp_drm_lease_v1`, `ext_session_lock_v1`, `zwlr_layer_shell_v1`, `zwlr_screencopy_v1`,
`zwlr_output_management_v1`, `zwp_tablet_v2`, `xwayland_shell_v1`, `wp_color_management_v1`,
`ext_workspace_v1` — wado is a single-output headless session that streams to one client. It
has no lock screen, no panels, no external capture clients, no second monitor to manage, and
no X11.

## Rule

Each protocol is its own change: a state field, a global, a `Handler` impl and a delegate.
**Do not batch them.** One protocol, one measurement of whether it changed anything.

## `zwp_linux_dmabuf_v1` is the one global created per session

Every other global is created in `Wado::new` under the rule "advertise unconditionally,
because a toolkit looks for its globals when it binds and one that appears later is one it
never asks for again." Dmabuf cannot follow it: the format list comes from
`renderer.dmabuf_formats()`, and `state.renderer` is `None` until `start_session`.

The rule has nothing to bite on here. **Every Wayland client in a wado session is spawned by
the compositor into a running session** (`headless.rs` launches them after the renderer
exists), so no client can bind before the global is created. It is created in `start_session`
and destroyed in `stop_session`, same shape as `output_global`.

Version 4 (with `DmabufFeedbackBuilder`) when `gpu.dev` is known — feedback is how a client is
told *which* GPU to allocate on — and version 3 (bare format list) on the surfaceless fallback,
which is still enough to get a client off shm. `gpu.dev` is the render node's `st_rdev`, which
is what a DRM node's `dev_t` is.

`DmabufHandler::dmabuf_imported` imports eagerly through `ImportDma` rather than deferring to
first use: a modifier the driver refuses is then a protocol error the client can still fall
back from, instead of a blank window at render time. **The `ImportNotifier` must be answered on
every path** — smithay logs "Compositor bug: Server ignored ImportNotifier" on drop and the
client waits forever.

### ⚠️ `windows=0` on the **positive** dmabuf line is not a contradiction

Seen 2026-09-12: `dmabuf path is live … format=AR24 … windows=0`. The import arrives **before**
the toplevel is mapped into `space`, so the count is legitimately zero at that instant.

The field exists for the **negative** branch, where `windows=0` means "no app was running, so
nothing could have asked" and the verdict is vacuous. On the positive branch it carries no
information and reads like a bug. Do not chase it; if anything, drop it from the positive line.

Also recorded: GTK4 (`snapshot`) takes the dmabuf path with **`AR24`** and a different modifier
from Chrome's `AB24`. Two independent toolkits, not one client's quirk — and GTK4 collects
presentation feedback too.

### ⚠️ Still unmeasured

The win is reasoned, not measured. To measure it: Chrome as the only client under sustained
motion, and compare the per-stage composite time from `timing.rs` before and after — **not**
`dec` or `kbps`, and only with the two `compositor session active` lines identical except for
this change. Five wrong conclusions in this project have come from comparing across configs.

## `zxdg_decoration_v1` answers ServerSide to everyone, on purpose

The protocol's whole value is the answer, so a wrong answer is worse than not implementing it.
Absent the global, toolkits assume client-side decorations — therefore replying `ClientSide` is a
**no-op**, and `ServerSide` is the only reply that changes anything. wado draws no decorations,
so `ServerSide` means **borderless**.

Chosen by the user 2026-09-12. Borderless suits this compositor rather than being a shortcut: the
viewer is usually a phone, where a CSD titlebar spends scarce vertical space on a strip with
untappable buttons; those buttons are already redundant (maximize/minimize/close/cycle arrive
over the remote input path); and dragging never needed a titlebar because long-press-drag is
`WindowDrag`.

A client that *requests* `ClientSide` is told `ServerSide` regardless — allowed, since the
compositor's configure is authoritative, and necessary, because an app insisting on its own
titlebar would reintroduce exactly the strip this removes. GTK keeps its header bar (application
content) and loses only the frame.

Implementation note: `send_pending_configure` only when `is_initial_configure_sent()`, because
`new_decoration` can arrive before the first commit and configuring an unmapped toplevel is a
protocol error.

## ⛔ `zwp_text_input_v3` stays blocked — but the user-facing problem is solved

The soft-keyboard gap that motivated it was closed client-side on 2026-09-12 (`8da467b`): a ⌨
button focuses a hidden `<input>`, which is the only thing that raises a phone keyboard.
`zwp_text_input_v3` remains a dead global without `zwp_input_method_v2` bound, and is still a
milestone rather than a protocol addition. What it would add beyond today: *app-initiated* keyboard
(tapping a text field in the app raises the keyboard by itself), and IME composition for scripts
that need it.

### ✅ Verified live, 2026-09-12 06:37 UTC

First session on the new daemon, Chrome the only client:
`activation request — raising and focusing` and
`presentation feedback answered … surfaces=1 seq=2382` both fired within 300 ms of Chrome's first
surface. `seq=2382` over ~40 s of session is 59.6 fps — the composite counter tracks wall clock.
`wp_content_type_v1` stayed silent (no video played); that question is still open.

⚠️ The positive presentation line counts callbacks **collected**, before `presented()` runs, so it
proves a client asked — not that the answer was accepted. The `clk_id` discard branch is
unreachable by construction (both sides derive from `Monotonic`), but if the clock ever becomes
configurable this count has to move after the answer.

## `wp_presentation`: the clock is the whole implementation

This protocol was parked twice — once on not knowing the panel refresh rate (closed by R9), and
once on the observation that **a wrong presentation timestamp is worse than no timestamp**,
because GTK and Chrome pace animations against it. Both halves of "wrong" were live risks:

**1. The clock.** `PresentationState::new::<D>(&dh, clk_id)` advertises a clock *id* to clients,
and a client compares the timestamps it receives against its own reading of that clock. Four
lines above the hook point sits `state.start_time.elapsed()` — time since daemon start — which
is correct for `send_frame` (frame callbacks take an arbitrary millisecond counter) and
catastrophic here: every frame would land one daemon-uptime in the past. The timestamp comes from
`Clock<Monotonic>::now()` on `Wado`, and the same `clock.id()` is what the global advertises.
Smithay enforces the pairing at the far end — `OutputPresentationFeedback::presented` derives
`clk_id` from the `Time<Kind>` it is handed and **discards** any callback whose id disagrees — so
a mismatch is silent, not a compile error.

**2. Unanswered callbacks.** Same contract as `ImportNotifier`: a callback that is collected and
never answered leaves the client waiting forever. Collection and the answer are in one block at
the end of `render_tick`, so no early return can sit between them, and
`SurfacePresentationFeedback` discards on `Drop` as a second net. The encode-failure path (which
downgrades the pipeline) reports **not presented** via the `composited` flag — the composite
happened, but nothing reached a viewer.

**3. The flags are empty, on purpose.** `Vsync`, `HwClock` and `HwCompletion` each assert a
property of a real scanout; `ZeroCopy` would claim the frame reached a display rather than an
encoder. Empty is the honest encoding of "software composite, timestamped as accurately as this
loop can". `Refresh::Fixed` comes from the output *mode* rather than a separate constant, so the
rate a client is told and the one `wl_output` advertises cannot drift apart.

`frame_seq` counts composites, not presentations, and resets in `start_session` — a client seeing
a gap after a dropped frame is being told the truth.

## `wp_content_type_v1` is advertised and read by nothing

Deliberate, and the same pattern as the fractional-scale bind log: the question *"would the
encoder benefit from knowing a video is playing?"* cannot be answered until we know whether any
real app ever says so. Invariant 7 fixes the encoder's tuning at session start, so there is no
per-surface knob to turn today regardless.

Logged **on change only** — a surface commits at the frame rate, so per-commit logging would bury
the signal — with an absent entry and an explicit `None` treated as the same non-event.

⚠️ If the encoder ever reads the hint, it must not trust it for anything a client could game. The
plausible use is pacing (`Video` on a full-screen surface means "expect sustained damage"), not
quality.

## `xdg_activation_v1` honours every token

The permissive end of the protocol. The usual reason to refuse a token is focus stealing between
mutually-untrusting apps, and that threat model does not exist here: **every client in a wado
session was spawned by the session itself.** What the protocol buys is the launcher — an app
started from the picker or the shell draws its first window behind whatever had focus unless
something raises it, and on a phone that means tapping a strip of window that may be fully
covered. Toolkits already pass `XDG_ACTIVATION_TOKEN` through `exec` for exactly this.

Two guards that are not optional: the surface is looked up in `space` first (a token can name a
subsurface, a popup, or a window that closed in between, and `focus_window` would then raise
nothing and focus a surface with no window), and the token is removed after one use — a token
that stays valid is a token that can raise a window long after the click that justified it.
