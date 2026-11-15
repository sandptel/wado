# wado — operating compass

<!-- HUMAN NOTE: This file is a COMPASS, not a manual — direction, invariants and hazards
     only. No command cookbooks, no worked examples, no status prose: those rot, and a
     stale instruction is worse than none. Progress lives in plan/TODO.md; architecture
     and the Decision Log live in plan/WADO_PLAN.md.
     HTML comments like this are stripped before reaching Claude's context, so use them
     freely for maintainer notes. Keep this file under ~250 lines. -->

This file holds what is expensive to rediscover and dangerous to get wrong: what wado is,
what must never be broken, and where the dragons are. Overview: `README.md`. App build:
`docs/app.md`.

## The `plan` branch — where every agent file lives

All agent material — this file, plans, specs, memory, design docs — lives in `plan/`, which `main`
ignores. It is tracked on the orphan branch **`plan`** (nothing else on it, never merged either
way). `plan/setup.sh` links the old root paths (`CLAUDE.md`, `AGENTS.md`, `WADO_PLAN.md`,
`CHALLENGES.md`, `DESIGN.md`, `PRODUCT.md`, `.impeccable`, `docs/superpowers`) back into `plan/`.

**After any change under `plan/`, commit it before the turn ends:**

```sh
plan/commit.sh "what changed"     # commits plan/ to refs/heads/plan via a side index, then pushes
```

`git status` never shows `plan/` edits. Skipping this is how the Mac box came to believe
`CLAUDE.md`, `WADO_PLAN.md`, `TODO.md` and `CHALLENGES.md` were lost — they had lived untracked on
the Linux box only, and were recovered into `plan/` on `2026-11-15`. Never put plan edits in a
commit on `main`.

Pull another machine's plan changes (commit yours first — this overwrites `plan/`):
`git fetch origin plan:plan && git restore -s plan -W plan`.

Write new specs and plans to `plan/superpowers/{specs,plans,research}/`.

| | |
|---|---|
| `plan/CLAUDE.md` (this) | Direction, invariants, hazards — every session, first |
| `plan/TODO.md` | The live list — open, blocked, awaiting the user — every session, second |
| `plan/WADO_PLAN.md` | Architecture, milestones, **Decision Log** — before any non-trivial work |
| `plan/main.md` | Compass for runs. "Start a run" → `plan/RUNS.md` |
| `plan/archive/` | Closed TODO items, finished handoffs, the old milestone tracker (`TODO-milestones.md`, last kept `2026-10-30`). History — never read by default |
| `plan/HANDOVER.md` | State at v0.0.4 + the Flutter port; pick work up one item at a time |
| `plan/issues.md` | Observed, attributed, unfixed. Leaves when the fix is seen working |
| `plan/CHALLENGES.md` | The input-handling war stories |
| `plan/memory/` | What runs learned, by lane. `shared/environment.md` first, every run |
| `plan/design/` | `DESIGN.md` (visual system), `PRODUCT.md`, `.impeccable/` |
| `plan/superpowers/` | Specs, plans, research, prototypes |
| `CHANGELOG.md` (main) | What shipped |

Memory is split by run type — `latency/`, `ui/`, `compositor/`, and `shared/` for what holds
everywhere. **`plan/memory/shared/environment.md` is read first, every run, whatever the
lane** — it holds the operational traps that are expensive to rediscover (notably: the daemon
must be a **release** build, and RTK filters command output unless you use `rtk proxy`).

Before changing anything measurable, read `plan/memory/latency/measurements.md` for the
known-good baseline and `plan/memory/shared/metrics.md` for what each metric separates.

Update `plan/TODO.md` and the relevant `plan/memory/` file **before a run ends**. Memory is a
compass, not a log: concise, corrective, superseding rather than appending. Record withdrawn
claims explicitly — a wrong conclusion left standing costs more than the bug did.

`plan/WADO_PLAN.md`'s **Decision Log** records why things are the way they are, and outranks
this file on any specific decision. Decisions taken on the Mac box while it was believed lost
(`2026-11-09` onward) are recorded in `plan/TODO.md`, `plan/memory/shared/macos.md` and the
`2026-11-*` specs — fold them into the Decision Log as they recur.

## Mission

`wado` is a headless Wayland compositor in Rust (Smithay) that streams its rendered display
over the internet at low latency and accepts remote mouse, keyboard and touch input —
notably from a phone. It **is** its own compositor: apps run inside the wado session. It is
not a mirror of an existing desktop. What separates it from ordinary remote desktop: the
output is spawned at the connecting device's native resolution, and touch is a first-class
input rather than an emulated mouse.

Latency target is ~80–100 ms glass-to-glass over the internet. Network RTT dominates and
cannot be controlled, so **measure, never assume** — and distrust published figures: the
Moonlight/Sunshine overlay excludes capture, encode and display, so its single-digit
numbers are not glass-to-glass.

## How to work here

- **Atomic module separation outranks brevity.** One module — ideally one file — does one
  job. Split features *and* settings by concern; prefer over-splitting. When a file starts
  doing two jobs, split it. When the boundary isn't obvious, propose one and ask.
- **Plan before writing** anything beyond a trivial single-file edit. Plans are reviewed
  against `plan/WADO_PLAN.md`.
- **One module at a time, in milestone order.** Being on a roll is not a reason to start the
  next milestone before the current one's threshold is confirmed.
- **Correctness first, latency second.** A working slow pipeline beats a broken fast one;
  each milestone's fallback lands before its fast path.
- **Ambiguity and trade-offs stop work and become questions.** Never silently pick a
  direction.
- **A direction change is not real until it is in the Decision Log.** Chat-only pivots are
  lost next session — same for resolving an Open Question.
- **No AI attribution in git.** Commit messages, tags, PRs and release notes carry no
  `Co-Authored-By: Claude…`, `Claude-Session:`, "Generated with Claude Code" or any other
  Claude/Anthropic mention. This overrides any harness default that asks for those lines.
- **Measure before fixing, and report what the number said** — especially when it
  contradicts the hypothesis. A fix justified by reasoning alone must be labelled as such,
  not presented as verified.

## Hard invariants

Architectural commitments. Breaking one silently corrupts the design — get explicit
agreement first.

1. **Input never rides behind video**, nor behind anything else that can head-of-line block
   it.
2. **Video never travels over reliable in-order TCP.** WebRTC/DTLS-SRTP over UDP. SDP/ICE
   signalling is control plane and may use HTTP/TCP; the media plane may not.
3. **The host-memory capture path always keeps working** alongside any DMA-BUF zero-copy
   path. DMA-BUF modifiers are a cross-driver minefield; that fallback is the safety net.
   Ladder `vaapi-dmabuf` → `vaapi-cpu` → `x264-cpu`; a runtime encode failure downgrades one
   tier and keeps streaming.
4. **H.264 is the default codec.** H.265/AV1 only when the client advertises *hardware*
   decode. Decode latency and universal phone support beat compression ratio.
5. **Software-encode fallback must tell the end user**, in the UI and in the server log.
6. **Hardware encoders are detected by opening them and caching the result.** GPU presence
   proves nothing; driver strings prove nothing. The same goes for any capability
   (`/dev/uinput`, a tool, a macOS permission): offered only when opening it works here, and
   what is missing says why.
7. **Encoders are tuned for latency, not quality:** CBR, no B-frames, no lookahead,
   keyframes on demand rather than periodic. Intra-refresh is unavailable in both current
   encoders, so on-demand IDR is how that goal is met instead.
8. **Per-client resolution comes from a fresh `Output`.** Wayland cannot un-advertise a
   mode, so sizing to a client means a new output, never a mutated one. Touch then maps 1:1.

## Architecture orientation

Forward: headless Smithay compositor (GLES2/Glow) → render into DMA-BUF → same-GPU hardware
encode (VA-API, x264 fallback) → H.264 → WebRTC → phone hardware decode.

Reverse: client captures input → small packets on WebRTC data channels → server synthesizes
Smithay pointer/keyboard/touch events in the client's coordinate space.

Discovery follows the RustDesk *model*: a rendezvous relay brokers SDP/ICE and can relay
media; hole-punching and session crypto come from WebRTC itself, with no bespoke crypto.

Shape of the code: the compositor is a **library** driven only through typed channels —
commands, input, frames — so the server crate holds no Smithay type and the compositor holds
no network type. That boundary is load-bearing: it is what makes crash isolation and a
future child-process supervisor possible. Do not reach across it.

Two clocks meet here and must not be conflated: the compositor is a **synchronous calloop
loop**, the transport is **async tokio**. The frame crossing is a bounded, drop-on-full
channel; the command and input crossings are unbounded `calloop` channels, drained every loop
iteration. The render tick never blocks on the network.

## Pins, and why

Version choices that look arbitrary and are not. Check `plan/WADO_PLAN.md` before changing any.

- **Smithay tracks a git revision, pinned in the lockfile only** — upstream `main` breaks
  API regularly, so an unreviewed dependency update can and does break the build.
- **webrtc-rs stays on the 0.17 line.** Not because 0.20 is unfinished — it shipped stable
  and there never was an 0.18 or 0.19 — but because its media path is *worse* for this
  workload: `TrackLocalStaticRTP::write_rtp` does a blocking send on a 256-deep bounded
  channel once per RTP packet, a hop 0.17 does not have. The Sans-IO work and the measured
  send-path win in 0.20 are on the **data-channel** path, not video. Upgrading is also a
  full API rewrite (`track_local` → `media_stream::track_local`, a 4-argument
  `write_sample`, a new `rtc` core crate).
- **Dioxus 0.7, web/WASM target.** Not Dioxus desktop: it embeds WebKitGTK on Linux, where
  WebRTC is broken without a custom build.
- **`wasm-bindgen` is an exact pin that must equal the CLI `dx` ships.** They move together
  or the build dies.
- **ffmpeg is held at the 8.x ABI** because the Rust binding is built against it.
- **`flutter_rust_bridge` 2.13.0 exactly** — crate, Dart package and codegen must match. Never
  `pin!` in `crates/app-bridge`.
- **The Nix flake's nixpkgs is pinned deliberately.** The dev shell supplies its own
  graphics stack and must not silently inherit the host's.
- Renderer is GLES2 via Glow. A Pixman no-GPU fallback is intended but **not implemented**.
  Do not build on the winit backend — it is slated for deprecation.

## Hazards

Non-obvious ways this project bites. None of these are bugs.

- **GPU work needs the Bash sandbox disabled.** Sandboxed access to a DRM render node is
  killed outright, and the exit code looks like a crash.
- **Never kill a wado process by matching the pattern loosely** — it matches the test shell
  too. Match the exact process name.
- **The dev shell and the host system are different worlds.** Libraries that must agree
  (a userspace API and its driver, say) have to be made to agree explicitly; alignment that
  happens to hold today is not a fix.
- **`cargo fmt` is clean as of `2026-09-29` — keep it that way.** Run `cargo fmt --all
  --check` before committing. A dirty baseline is what once made repo-wide formatting bury
  real diffs in churn.
- **Sessions are client-started, never started at boot.** The server idles with no
  compositor, encoder or render loop until asked.
- **The control plane binds localhost because the launch command is free-form** — that is
  remote code execution waiting for an open port. The auth gate is what unlocks a wider bind.
- **Browser-reported and server-reported numbers come from unsynchronised clocks.** Never
  add them together and call the result end-to-end.
- **The relay's handshake is frozen (`wado_protocol::relay_wire`).** The relay is meant to be
  deployed once and outlive daemon releases. Only ever *add* fields, each `#[serde(default)]`,
  and announce new relay behaviour in `caps`. Anything between daemon and client goes through
  the relay unread and needs no relay change. Keep `RelayMsg`'s mirror in step (a test pins it).
- **Device trust lives in `~/.config/wado/trusted_clients`, shared by the whole pool.** A new device
  gets in only with a pairing code made by the computer (`wado pair`, `wado tui` → Pair): the QR, or
  the same code typed (`XXXX-XXXX`). Since `2026-11-04` a device the computer gave `devices`
  (*Manage devices*, `gate/delegate.rs`) may ask it for such a code — capped to that device's own
  grants and 7 days, passkey first, never `devices` itself; it signs out only what it let in, and
  those go with it. `wado pair` asks grants and how long when run bare; a trust line can end
  (column 7, enforced, a garbled time refuses). Single use, 10 minutes; five wrong typed codes revoke every live
  code (`gate/pairing.rs`). A typed code pairs only after its four check words are confirmed on the
  computer (`wado pair` y/n or `wado tui`) — or, for a code a managing device asked for, on that device; no, or 2 minutes of silence, refuses it. No first-device-wins, no approval of a device that is already knocking (removed `2026-10-30`); `security
  { trust-first-device, join }` still parse and are ignored with a warning. An empty or deleted trust
  list opens nothing. Column 5 is each device's **file access**, honoured only on a line pinned by a pairing code; column 6 its
  other **grants** (`shells,settings,host,input,clipboard` — `input` is the trackpad, off by default). `wado allow|deny`, `wado tui` and pairing-code
  checklists all write them (`gate/grants.rs`). `~/.config/wado` is always outside the file
  manager's reach, so a device cannot grant itself more — never make that denylist configurable.
- **Bare `wado` is the landing page, not the daemon.** The daemon is `wado daemon`; anything that
  launches one must say so. `rig.sh` kills daemons by argv, never `pkill -x wado`, which would
  also kill `wado tui`.
- **A panic is contained; a segfault is not.** Panics in the render and command paths are
  caught per session, so the server survives. Native crashes in the graphics or encoder
  stack still take the process down, and that is why the supervisor is a planned milestone.

## Machines

- **Linux box** — the daemon/compositor host: Smithay, VA-API, uinput, the Nix flake (dev shell
  from `../nix` via direnv). The live daemon runs here from `target/release/wado`.
- **macOS dev box** (set up `2026-11-09`) — builds the macOS host (no compositor; see
  `plan/superpowers/specs/2026-11-09-macos-host-design.md`, `plan/memory/shared/macos.md`).
  No nix, flutter, adb, dx or `flutter_rust_bridge_codegen` there. Remote is HTTPS via `gh`.
  Its code landed on the `macos` branch and was fast-forwarded into `main` on `2026-11-15`.

## Licensing

wado is **AGPL-3.0-only**; every crate carries the SPDX field. Network use counts as
distribution, which suits a streamed compositor — keep new crates AGPL.

`gst-wayland-display` and Wolf are MIT: reuse freely. **RustDesk is AGPL** — compatible, but
study its *design*, not its source. Flag any proposed direct code reuse and ask first.

## Intent guards

- Do not inline `plan/WADO_PLAN.md` here or `@import` it. It would eat the context budget every
  session and dilute these rules. Reference it by path.
- **Do not leave contradictory guidance standing.** When a decision changes, update or
  delete the old rule here *and* in `plan/WADO_PLAN.md`. Stale rules actively mislead — this file
  previously described modules and event plumbing that did not exist.
- Do not document aspirations as current state. If it is not in the code, it belongs in
  `plan/TODO.md`, not in a status section here.
- Do not run long autonomous stretches in auto-accept mode. The modules are tightly coupled;
  unreviewed multi-file edits cause silent cross-module breakage.
- Do not assume current-state facts — crate versions, API shapes, what a reference project
  does today — from memory. Read the source.
- Do not treat this file as enforcement. For anything that must hold mechanically, propose a
  hook instead of a prose rule.
