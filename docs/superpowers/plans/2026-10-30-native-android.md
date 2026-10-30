# Native Android App Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the WebView client inside the Android app with a fully native Jetpack Compose app. The app is driven by one Rust client core that the F8 desktop app also links. The web client stays for browsers and iOS.

> **2026-10-31:** the framework is now Compose Multiplatform for every platform, Android first. Keep screens free of `android.*` imports where a common API exists; platform features go behind `expect`/`actual`.

**Architecture:** A new pure-Rust crate `crates/client-core` holds the client protocol:
- the relay link
- the E2E handshake and seal
- pairing
- per-computer links on seatless control rooms
- feature codecs and state machines
- lane pacing
- reconnect

`crates/mobile-core` is its UniFFI façade for Kotlin. Compose draws every screen. Video and the data channels use Google's libwebrtc for Android (Stream's prebuilt), with MediaCodec hardware decode into a SurfaceView. The Rust core carries every SDP and ICE message sealed. The app ships in eight phases. Each phase lands native screens and leaves the remaining screens to the WebView. The WebView is deleted last.

**Tech Stack:**
- Rust:
  - `wado-protocol`
  - `ring` 0.17: the daemon's own primitives
  - `tokio` 1.53
  - `tokio-tungstenite` 0.24 with rustls and webpki roots: the daemon's own TLS stack
  - UniFFI 0.32: proc-macros, library mode
- Kotlin and Compose:
  - AGP 9.5, Kotlin 2.4.21
  - Compose BOM 2026.09.00
  - Navigation 3
  - kotlinx-coroutines
- Libraries:
  - `io.getstream:stream-webrtc-android`
  - Media3 ExoPlayer
  - Coil 3 and Telephoto
  - Termux `terminal-emulator` and `terminal-view`, vendored
  - CameraX and ZXing (existing)
  - UnifiedPush (existing)

**Spec:** Part A of this document is the design. It argues from:
- the Decision Log entries of `2026-10-30`:
  - "The Android app goes fully native"
  - "The native Android app is Jetpack Compose"
  - "Parallel use by default"
  - "Entering a session is always the user's choice"
  - "Every action shows its state"
- `PRODUCT.md`
- `DESIGN.md`
- these specs and research:
  - `docs/superpowers/specs/2026-10-30-computer-page-design.md`
  - `docs/superpowers/specs/2026-10-30-pocket-console-design.md`
  - `docs/superpowers/specs/2026-10-30-virtual-input-design.md`
  - `docs/superpowers/specs/2026-10-30-multi-client-design.md`
  - `docs/superpowers/research/2026-10-30-touchpad-feel.md`

Executors read Part A before any task.

---

# Part A — Design

## A1. What changes and what does not

| Holds | Changes |
|---|---|
| Wire protocol, relay handshake (frozen), envelope crypto, daemon behaviour, except the additive items marked **daemon** below | The app stops hosting the Dioxus client. Every screen is Compose. |
| The web client for browsers and iOS, unchanged | The client protocol gets a second implementation, in Rust, in `crates/client-core`. The JS client stays the reference, and the shared vectors pin the two together. |
| The invariants in `CLAUDE.md`. Specifically: input never behind video (1), video over UDP WebRTC (2), H.264 by default (4), software-encode notice (5), fresh Output per size (8) | The app's device identity. The WebView's Ed25519 key is non-extractable (`js/e2e_keys.js`), so the native core is a **new device** and each computer is re-paired once (A9, R1). |
| DESIGN.md: base16 roles, two curves, duration tokens, 44 dp targets, Stage Black | Motion runs in Compose: `SharedTransitionLayout` and predictive back, in place of hand-rolled CSS FLIP. |

## A2. The client core (`crates/client-core`), shared with desktop

**Decision: split the core in two.**
- `crates/client-core` (`wado-client-core`) is pure Rust, has no FFI, and is tested on the host.
- `crates/mobile-core` stays the UniFFI cdylib and becomes a thin façade over client-core. It keeps the existing Android-specific helpers: `pad.rs`, `webauthn.rs` and `words.rs` (the last two move into client-core in P2 and P3, when their callers do).
- The F8 desktop app (`experimental/desktop-app`, which today has no protocol code) depends on `wado-client-core` directly, with no UniFFI and no JNA.
- This refines the `2026-10-30` entry ("mobile-core … to become the client core") and needs a Decision Log line.

### Link model: parallel use from the start

There is one **seatless control room per computer** (`/join/:rid?client=&name=&control=1&strict=1`, the relay's `control` and `room_strict` caps). Every feature the daemon serves on control rooms rides it, and the core never asks for the seat to use a feature.

Today the daemon serves these on control rooms (`crates/server/src/control/policy.rs`):
- `HostGet`
- `ConfigGet`
- `HostDo`
- `Clip*`
- `PadIn{Actions|Run}`

Each later phase carries the **daemon** task that moves its feature onto control rooms. This is HANDOVER item 0b, already decided. The session uses a second room with `RoomHello{kind:"view"}` for join and co-control, and the daemon's per-feature `holds/` for the exclusive touchpad and keyboard.

**Exception, transitional:** pairing. The daemon refuses pairing on a control room by design ("only a device already trusted … no pairing, ever", `control/mod.rs:10`). The core pairs on the seat join with `&pair=1`, exactly as `relay_link.js:241` does, and leaves with close code `4001` as soon as it gets `e2e_ok`. No feature ever uses the seat.

### Modules (one job each)

```
crates/client-core/src/
  lib.rs                 re-exports; no logic
  e2e/keys.rs            HKDF key schedule, pair_mac, typed_mac, k_sas, SAS bytes (pure)
  e2e/seal.rs            Sealer/Opener: AES-256-GCM, strict per-direction counters, {"type":"sealed","n","c"}
  e2e/handshake.rs       client half: hello → verify reply (host sig over H(T1), pin) → finish → ok|sas|fail
  identity.rs            this device: Ed25519 seed + client_key; persisted only through SecretBox
  secret.rs              trait SecretBox { wrap, unwrap } — the platform's keystore
  pins.rs                host pin per Remote ID: {pin, from: qr|first-use}; mismatch = refuse
  profiles.rs            saved computers (key, name, relay_url, remote_id, os cache, hue, last_used); atomic JSON file
  pair/link.rs           parse a pairing link (?relay&id&pair&hk, wado://pair), normalise typed codes XXXX-XXXX
  pair/flow.rs           one pairing ceremony: QR or typed (check words), on the seat join, then 4001
  relay/url.rs           join URLs (seat+pair, control room, view room); ws/wss from relay_url
  relay/transport.rs     trait Transport + tokio-tungstenite impl (rustls, webpki roots)
  relay/frames.rs        classify an inbound frame: relay handshake (WireMsg), e2e, sealed
  link/state.rs          one computer's link state machine (pure: event in → actions out)
  link/backoff.rs        reconnect delays (pure)
  link/outbox.rs         client-side lane: Urgent/Bulk/Spare, wado_protocol::lane::Pace, seal at dequeue
  link/driver.rs         runs one link: transport + state + outbox on the tokio runtime
  feeds/host.rs          HostGet/HostState → HostView (name, os, logo, desktop, uptime, media, volume, brightness…)
  feeds/sessions.rs      SessionsRequest → session rows (count, first app, lead, participants)
  os_logo.rs             os-release ID / ID_LIKE → logo slug, with fallbacks (pure; shared with desktop)
  request.rs             request ids, additive `ack`, per-request deadline → Answer events (the feedback primitive's wire half)
  core.rs                Core: owns the runtime, profiles, links; command queue in, event queue out
  event.rs               CoreEvent and its view records
  -- later phases (outline in Part B) --
  config/{view,set}.rs   ConfigState → typed views; ConfigSet builders                        (P2)
  passkey/{flow,msgs}.rs passkey ceremony messages, unlock state, lifetime                   (P2)
  host/actions.rs        HostDo builders (volume, brightness, media, keep awake, Wi-Fi, BT)   (P2)
  media/signal.rs        SDP/ICE over the sealed link; trickle timing rules (F4)            (P3/P6)
  pad/{link,game,holds}.rs pad link signalling, gamepad messages, holds and take over       (P3)
  layout/{model,place,gamepad_variants,touchpad_variants}.rs one layout model for gamepad and touchpad, placePx (spec §9) (P3)
  pad/compose.rs         touchpad controls → pad messages (surface contacts, strip/wheel two-finger synthesis, clickbar, boost, ring) (P3)
  files/{ops,xfer,stream,kinds}.rs file ops, transfers, Stream+Credit, kind classification (P4)
  shell/{pty,list}.rs    Pty* messages, per-device shells                                    (P5)
  session/{intent,input,presence,stats}.rs "left means left", InputEvent codecs, ghost cursors (P6)
  clip/{share,history}.rs clipboard hub client side (echo hashes, sensitive)                (P7)
  push.rs, claude.rs, linktest.rs                                                           (P7)
```

### API shape across UniFFI

- **Commands** are plain methods that enqueue and return at once, for example `watch(keys)`, `pair(link) -> OpId` and `host_do(key, action) -> OpId`.
- **State** is pulled: `async fn next_event() -> CoreEvent`, which Kotlin sees as a `suspend fun`.
  - The core never calls into the JVM from its own threads.
  - Backpressure is natural: if the UI stalls, events wait in a bounded queue that coalesces per (computer, feed), keeping the newest. They are never dropped silently.
- **Hot paths are synchronous pure calls:** input encoding, pad pacing, file-frame parsing. Kotlin calls them on its own input or IO thread and sends the bytes on a libwebrtc `DataChannel`. Nothing on the input path waits on the tokio runtime (invariant 1).

### Seal at dequeue

The envelope's counters are strict (`envelope.rs`: a counter that is not exactly the next one closes the link). The outbox therefore holds **plaintext** with its priority class and seals each frame as it leaves. Urgent frames (signalling, verbs, answers) jump Bulk and Spare frames before sealing. Budget: the daemon's own client-to-relay numbers, 56 KiB/s with a 1.25 MiB burst (`wado_protocol::lane`).

## A3. Media stack: libwebrtc for Android (decided; confirmed here)

| | **A: Google libwebrtc, Stream prebuilt** | B: webrtc-rs 0.17 in the core + own depacketiser and jitter buffer + NDK MediaCodec | C1: str0m (Sans-IO) in the core | C2: LiveKit `libwebrtc` Rust crate |
|---|---|---|---|---|
| Latency | Same engine as the WebView's Chromium, minus the WebView compositor. Jitter-buffer floor is settable (`RtpReceiver.setJitterBufferMinimumDelay`). Honours the playout-delay extension if the daemon adds it. At least on par by construction. | Could be lower (render on frame-complete), but unproven. | As B. | As A. |
| Robustness | NACK, RTX, PLI on decode error, FIR, ULPFEC/FlexFEC, TWCC and GCC, years of tuning. | We write frame assembly, jitter, PLI-on-error and A/V sync ourselves. | NACK and keyframe request built in; the rest is ours. | As A. |
| HW decode | `HardwareVideoDecoderFactory`, MediaCodec straight to a `SurfaceViewRenderer`. H.264 everywhere; H.265 and AV1 when the device reports them. | `AMediaCodec` to an `ANativeWindow`, ours to get right per vendor. | As B. | Android HW path through Rust is uncharted. |
| Audio | Opus decode, AAudio/OpenSL output and A/V sync included. | We add libopus, AAudio and sync. | As B. | Included. |
| Data channels | SCTP, per-channel `ordered`/`maxRetransmits`. | webrtc-rs SCTP. | str0m SCTP. | Yes. |
| Size (arm64) | About 10–12 MB `.so` (estimate; measured in P3) | About 4–6 MB added to the core | About 3–5 MB | Over 12 MB plus the Rust layer |
| Maintenance | Stream publishes on Maven Central (1.3.10, `2026-09-11`), tracking upstream. Apache-2.0 wrapper over BSD-3 libwebrtc, **both AGPL-3.0 compatible**. Fallback: build libwebrtc from source with the same API. | webrtc-rs 0.17 is pinned server-side; the receive path is untried on Android. | Active, small. | Couples us to LiveKit's fork. |
| Fit with 1, 2, 4 and desktop | 1: input channels on SCTP, independent of RTP; pad link on its own PC. 2: DTLS-SRTP/UDP. 4: codec preferences put H.264 first and add H.265/AV1 only when `MediaCodecInfo.isHardwareAccelerated`. Desktop: not shared, through the media port below. | All hold; shareable with desktop. | All hold; shareable. | All hold; shareable, but heavy. |

**Pick A.** Five reasons:
1. It is the same engine that gives today's WebView its glass-to-glass, so parity is the floor rather than a hope.
2. Loss recovery, audio and A/V sync come with it. Under B or C those are months of work, and each is a latency or robustness risk.
3. Hardware decode into a `SurfaceView` is the shortest path from MediaCodec to the display. SurfaceView is composited by SurfaceFlinger as an overlay.
4. Licences are clean.
5. The data channels, the pad link and the files PC share one stack with video, so the APK carries one WebRTC.

B and C stay possible behind the **media port**, `media/signal.rs`. The core speaks only SDP strings, ICE candidate strings and channel bytes, so a desktop engine (webrtc-rs, per F8's plan) plugs into the same core. Per the user's "optimisations as options" rule, a future native-Rust receive path would be a switch on Display & stream, never a replacement.

**Pin:**
- `io.getstream:stream-webrtc-android` at an exact version. Record the libwebrtc milestone the AAR reports in `docs/android.md` when it lands (P3).
- `abiFilters` arm64-v8a and x86_64 (emulator).
- R8 keep rules for `org.webrtc.**`.

**Signalling through the core.** The client offers and the daemon answers (`viewers/pc.rs:336`). For a session:
1. Kotlin creates the PC.
2. Kotlin adds `recvonly` video and audio transceivers.
3. Kotlin creates the channels:
   - `wado-input`: ordered, reliable
   - `wado-motion`: ordered, `maxRetransmits 0`
   - `wado-fast`: unordered, `maxRetransmits 0`
   - `wado-presence`: unordered, `maxRetransmits 0`
4. Kotlin calls `core.offer(key, sdp)`.
5. The core seals `SdpOffer` on the view room.
6. `SdpAnswer` and `IceCandidate` come back as events, and Kotlin applies them.
7. Local candidates go out through `core.ice(key, cand)`. The core holds them by F4's rule: send once a srflx candidate is in, or 700 ms after the first host candidate, then trickle the rest.

The pad link (`pad-touch`, `pad-keys`, signalled by `PadOffer`/`PadAnswer`) and the files PC (`files`, signalled by `FilesOffer`/`FilesAnswer`) are separate PCs, signalled the same way.

**ICE on every interface.** This answers WebView defect 7 and HANDOVER item 1.
- `RTCConfiguration`: `candidateNetworkPolicy = ALL`, `continualGatheringPolicy = GATHER_CONTINUALLY`, `PeerConnectionFactory.Options.networkIgnoreMask = 0`.
- STUN set: the daemon's (`ice.rs`).
- P3 verifies on a real phone acting as the computer's hotspot that a candidate on the tether interface is gathered.
- If libwebrtc's Android network monitor still hides the tether interface, TURN on the relay (`ice.rs` already has the hook) is the honest fallback, and the link pill says "Relayed".

**Latency knobs** (each measured, each an option):
- `setJitterBufferMinimumDelay(0.0)` on the video receiver.
- **daemon:** register the `playout-delay` header extension (min 0, max 0) behind a Display & stream switch.
- A low-latency MediaCodec hint (`KEY_LOW_LATENCY` on API 30+), if the factory exposes it. Otherwise this is noted as a candidate for a patched decoder factory.

## A4. Terminal: Termux's `terminal-emulator` and `terminal-view`, vendored

- **Licence:** Apache-2.0. The rest of termux-app is GPLv3; these two modules are explicitly Apache-2.0. Both licences are AGPL-compatible. Keep their LICENSE and NOTICE in the vendored modules and on About.
- **Why not a Compose canvas renderer over `alacritty_terminal`:** Android soft-keyboard handling (IME composition, key repeat, Ctrl and Alt via extra keys, selection handles, TalkBack) is what makes a terminal usable on a phone, and Termux has years of that. A Compose renderer would have to rebuild all of it.
- **Why vendor rather than depend:** `TerminalView.attachSession` takes a `TerminalSession`, which spawns a local process through JNI.
  - The plan vendors both modules as `android/terminal-emulator/` and `android/terminal-view/`.
  - It adds one class, `RemoteTerminalSession`, which feeds `TerminalEmulator.append(bytes)` from the core's `PtyOutput` and writes keys through `core.pty_input`.
  - It drops the JNI subprocess code.
  - The patch is kept as a small, separate commit so upstream updates stay mergeable.
- Theme: base16 into `TerminalColors`, with JetBrains Mono.
- Desktop is free to use `alacritty_terminal` over the same `shell/` core module.

## A5. Native file viewer: same kinds as `js/viewer/kinds.js`

The kind classification moves into the core (`files/kinds.rs`, a port of the `KINDS`/`BARE`/`MIME` tables), so web, Android and desktop agree.

| Kind | Native |
|---|---|
| image (png jpg gif webp avif bmp ico svg heic heif) | Coil 3 decode. Telephoto `ZoomableAsyncImage` for pinch, double-tap and pan with subsampling. SVG via Coil's SVG decoder. On Stage Black. |
| video, audio (any ffmpeg format) | Media3 ExoPlayer over a custom `DataSource` fed by `FileReq::Stream` (fragmented MP4, the computer remuxes or transcodes) with `Credit` flow control. A seek opens a new `Stream{start}`. Audio uses the same path with a `MediaSession` (lock screen). |
| pdf | `android.graphics.pdf.PdfRenderer` on the cached file. Pages render lazily into a zoomable column. |
| text and code | The first `limit` bytes (the module's `head` fetch), in a `LazyColumn` of lines, JetBrains Mono, with Wrap and Source toggles. |
| markdown | `pulldown-cmark` in the core (MIT) → a small block AST → Compose. Shared with desktop. |
| json | `serde_json` in the core → a tree of nodes → collapsible rows. |
| csv / tsv | The `csv` crate in the core → rows → a sticky-header grid. |
| archive | `FileReq::Entries` (the daemon lists it) → rows. |
| font | `Typeface.createFromFile`, specimen text at three sizes. |
| info (everything else) | The honest card: type, size and modified, plus Download and Open on the computer. |

The lane rule holds: on the relay's limited lane, files over 1 MB and all media wait for a direct path, and the viewer says so.

## A6. Settings: native screens over the same messages

`ConfigState`/`ConfigSet` (`wado_protocol::config`) are decoded by the core into typed per-page views (`config/view.rs`). Kotlin never parses config JSON. Pages:
- **Computer:** Display & session, Sound & devices, Connection & trust, Virtual input (Touchpad, Keyboard, Gamepad).
- **On this phone:** Appearance, Input feel, Gamepad, Quick tiles, Notifications & background, Diagnostics, About.

The pages follow the same component-per-page rule as the web (`computer/pages/*`, `home/pages/*`). The Apply bar and the "applies live / on Apply / host" badges work the same way.

## A7. UX

### Navigation: one stack, restored on resume

- Navigation 3 back stack of serializable keys: `Home → Computer(key) → Page(key, page) → Session(key)`.
- It is saved with `rememberNavBackStack`, so process death and resume restore the page the user was on. This answers WebView defect 6.
- **The session is entered only by the user's pick.** A late `session_started`, a retry or a resume never pushes `Session`. The core answers a late start with `session_detach` (`session/intent.rs`, the port of `js/session_intent.js`). Back from the session always pops to the Computer page and stops this device's retries. This answers WebView defect 5.
- **Predictive back** scrubs the reverse transition for every pop (M4/M17 driven by `PredictiveBackHandler` progress).

### Home (one native home; no middle web home)

Top to bottom:
1. The wordmark.
2. **Computers**.
3. **Running**: sessions, each naming its computer.
4. **On this phone**.
5. **Widgets** (P7).
6. "Add a computer".

The "Everything else" door goes away in P8. Until then it reads "More (web)" and sits last.

**Computer card**, as the spec's §3 card, with the OS logo:
- An 80 dp card on surface-1 with a line hairline, r3.
- A **46 dp disc** in the computer's hue holding its **OS logo**, monochrome in `bright`.
- The name in Bricolage 17/700.
- OS · compositor in mono.
- **Link words** with the tone dot: ripple on Direct samples, breathe on Relayed/Reconnecting, still on Offline.
- "· N sessions".
- A trailing 48 dp **▶**: accent tint, or solid accent with glow when a session runs. It is disabled when the computer is offline.
- Tap → Computer page by shared element (M2).

**Natural quick controls, one gesture away** (P2, with the Computer page's Quick):
- **Swipe up on a card** (or long-press, or the TalkBack action "Quick controls") unfolds it in place into a **quick lip**. The card's height springs open (`--dur-sheet`, `Motion.Spring`) and holds:
  - Volume (thick slider plus mute)
  - Brightness (stepped)
  - Now playing (⏮ ⏯ ⏭ and title)
  - **Keep awake** toggle tile
  - **Touchpad** door (with "In use by X — Take over?" when held)
- While the lip is open, the phone's volume keys drive that computer.
- When something is playing, the folded card shows a single ⏯ glyph by the ▶, so pausing music is one tap from home.
- Every control hides with a reason (missing grant or capability, honest per DESIGN) rather than disappearing.

**OS logos:**
- **Source:** Simple Icons (CC0 project). Some icons carry their own licence field, so `scripts/os-logos.mjs` pins a Simple Icons release, copies only the listed slugs' path data into `res/drawable/os_<slug>.xml`, **fails if any icon's `license` field is set to anything but CC0 or if a slug is missing**, and writes each brand's `guidelines` URL into `assets/licenses/os-logos.txt` (shown on About).
- **Trademark guidance:** the logos are used only to identify the computer's OS (nominative use), unmodified in shape and never as wado's own mark. They are rendered **monochrome in a base16 role**. That also obeys the Sixteen Slots Rule (no brand hex).
- **Mapping** (`os_logo.rs`, pure):
  1. Exact `ID`, then each `ID_LIKE` entry in order, then the `linux` (Tux) slug.
  2. Exact IDs: `nixos` `arch` `endeavouros` `manjaro` `debian` `ubuntu` `pop`→`popos` `linuxmint` `fedora` `opensuse-*`→`opensuse` `gentoo` `alpine`→`alpinelinux` `elementary` `kali`→`kalilinux` `void`→`voidlinux` `centos` `rocky`→`rockylinux` `almalinux` `rhel`→`redhat` `zorin` `artix`→`artixlinux` `mx`→`mxlinux` `deepin` `solus`, plus `steamos`. Each slug is verified by the script.
  3. **daemon (additive):** `SysInfo.os_like: Vec<String>` with `#[serde(default)]`, read from `ID_LIKE`. Today only `os_id` is sent (`server/src/platform/linux/sysinfo.rs`).
  4. The core caches `os_id`/`os_like` in the profile, so an offline card keeps its logo.

### "On this phone" (native home section)

| Row | Holds | Phase |
|---|---|---|
| Appearance | Scheme (the base16 list from `crates/client/src/schemes.json`, copied at build), accent slot, corner style, motion (full / reduced / off), **Text size** (85–150 % in 5 % steps, or Follow phone) and **Interface size** (Compact 0.9 · Cozy 1 · Comfy 1.1 · Large 1.2), with a live sample that applies on release | P1 |
| Notifications & background | Stay connected, clipboard switches, volume keys, notification channels, push distributor | P1 (existing switches moved), P7 (push) |
| Diagnostics | Per-computer link state and hops, relay caps, core and libwebrtc versions, the link test (P7) | P1 |
| Input feel | Finger mode, scroll speed, natural scroll for the session | P6 |
| Gamepad | Physical gamepads and phone haptics | P3 |
| Quick tiles | Which tiles the session's control centre shows | P6 |
| About | Licences: AGPL, Termux modules, logos, Stream/WebRTC, Media3 | P1, grows |

**The Size model in Compose:**
- A `SizeScope` composable wraps **pages and sheets only**, never the stage, console, pad or gamepad.
- It provides `LocalDensity` as `Density(density × interface, fontScale × text)`, where `text = follow ? systemFontScale : chosen`.
- Targets stay at least 44 dp **on the glass**: `Modifier.tap()` uses `44.dp / interface`.
- This is DESIGN.md's Size section with Android's own pair, minus the WebView's `textZoom` trap.
- The local value is the source of truth. It is mirrored to each computer's `device.<k>.size` best-effort, so the web part agrees until P8.

### Per-computer pages (P2)

These follow the computer-page spec §3:
1. Hero: disc with logo, name, meta strip (Link / Computer / Sessions), and the hop bar while dialling.
2. Big actions: Desktop · **Virtual input** group (heading plus Touchpad / Keyboard / Gamepad doors) · Files · Shell.
3. Now playing.
4. Quick: Volume, Brightness, Keep awake, Wi-Fi with its confirm, Bluetooth.
5. **Passkey card**.
6. **Sessions**: join, Start my own, End, Take over for size.
7. Clipboard (P7).
8. Claude (P7).
9. The four **This computer** rows.

Sub-pages carry the hue-dot eyebrow.

### Motion (DESIGN.md tokens; no third curve)

- `Motion.kt` gains `DUR_HERO 520`, `DUR_PRESS 120`, `DUR_STAGE 620`, `DUR_STAGE_OUT 520` and the reduced set 120 / 200 / 160 / 120 / 0 / 160 / 160.
- It also gains a user **motion level**: full / reduced / off, AND-ed with the system's animator scale.
- **M2/M4 card ⇄ Computer page:**
  - `SharedTransitionLayout` with `sharedBounds` on card → hero surface and `sharedElement` on disc and name, at 520 ms `Motion.Spring`.
  - Home recedes to scale .94 and fades.
  - Exit runs at 420 ms.
  - Scrolled more than 40 dp → crossfade instead (the spec's rule).
  - Shared elements in Compose stay interactive: the destination takes touches from frame one, as the spec's FLIP requirement asks.
- **M17 sub-pages:** push from 100 % to 0 with the page behind drifting −24 % and fading, at 480 / 340 ms Spring.
- **Predictive back** drives every one of these by gesture progress, and commits or cancels on Spring.
- **S1–S13 console ⇄ cockpit** (P6): the picture is one `graphicsLayer` transform. Buttons are the same composables in both arrangements, placed by `LookaheadScope` + `animateBounds`, which is Compose's FLIP. Plates cross-fade. Waves are 24 ms apart.
- **Direct manipulation** (pad, sliders, dragged widgets) follows the finger with no easing.
- Animate only `graphicsLayer` properties (translation, scale, alpha), never layout size per frame. This is the Compose form of "transform/opacity only".

### Virtual input: one layout model for gamepad and touchpad (P3; HANDOVER 4b)

The user asked for **touchpad variants**, built like the gamepad's layouts, with the same editor and an animated, "surreal" feel. The native app builds them first. The web follows from the same model.

**One model.** `wado_protocol::gamepad::Layout` (virtual-input spec §9.1; `js/gpad/model.js` documents the shape today) is generalised to `wado_protocol::layout::Layout`. Its new fields:
- `kind: "gamepad" | "touchpad"`
- control kinds for the touchpad:
  - `surface`: pointer area; MT contacts to the uinput touchpad
  - `strip`: a scroll axis, `axis: "v" | "h"`
  - `clickbar`: Left / Right buttons
  - `wheel`: a scroll area
  - `boost`: a hold-to-change pointer speed button
  - `ring`: volume or seek, `does: "volume" | "seek"`
- a `look` block: `trail` (off / glow), `rings` (on / off), `texture` (none / grid / dots), `shape`, `op`

The fields stay as before: `v`, `id`, `name`, `base`, `controls` (overrides only), edge-anchored `l`/`p` positions, and `placePx` (§9.3). Existing gamepad JSON reads unchanged: `kind` defaults to `"gamepad"`, a test pins this.

Storage mirrors `gamepad { mine … }`:
- `device "<k>" { trackpad { layout "<id>"; mine <id> "<name>" "<base>" "<json diff>" } }`
- At most 8 layouts, each at most 4 KB, self-settable.
- The `trackpad` block keeps its name, per the virtual-input decision.

The model, the built-ins and `placePx` live in the core (`layout/{model,place,touchpad_variants,gamepad_variants}.rs`) and are shared with desktop. Compose has **one renderer and one editor** for both kinds.

**Built-in touchpad variants.** Names carry no brand words.

| Variant | Controls | How it reaches the computer (honest to the touchpad research) |
|---|---|---|
| **Plain** | one `surface` | today's MT frames |
| **Laptop** | `surface` + `clickbar` (Left / Right) | **daemon:** pad message `Button{which:"left"\|"right", down}` → `BTN_LEFT`/`BTN_RIGHT` on the touchpad device (research P0-5). It clicks with tap-to-click off. The bar's own touches never become contacts. |
| **Edge scroll** | `surface` + right `strip(v)` + bottom `strip(h)` | The strip **synthesises a two-finger contact pair** moving 1:1 with the finger, so libinput's own two-finger scroll applies, with the computer's scroll speed and natural-scroll setting. Verified with `libinput debug-events` (research M4). If libinput misreads it, the fallback is a **daemon** `Wheel{dx,dy}` on a hi-res wheel sibling device. |
| **Split** | `surface` + `wheel` area | as Edge scroll |
| **Precision** | a smaller `surface` + `boost` | **Speed stays the computer's** (research rec. 3). Holding Boost sends `SpeedHold{factor}`; the daemon applies it to Hyprland's `device[wado-trackpad]` sensitivity (cap `pad-hypr`, F11) and restores it on release or silence. The phone never scales coordinates. |
| **Media** | `surface` + `ring` | The ring's angle → `HostDo` volume (or seek on the computer's player). Shared one-shot host actions; no hold needed. |

**Feel** (research §0 and §7, all native):
- One frame per input batch, stamped with `MotionEvent.eventTime`; historical samples read.
- Wi-Fi low-latency lock while open.
- `MT_TOOL_PALM` from touch major (research rec. 5).
- The tap mirror at `1.3 mm × px-per-mm`.
- Haptics on thresholds only: `GESTURE_START` on axis lock, `GESTURE_THRESHOLD_ACTIVATE`/`DEACTIVATE` crossing the commit notch, `CONFIRM`/`REJECT` on actions. No tick trains.
- The host gesture map is mirrored with truthful labels ("3 · Workspaces →").

**The surreal layer** (motion-design: three layers, inside DESIGN.md's two curves and named exceptions):

| Layer | Element | Motion | Rule |
|---|---|---|---|
| Primary | **Contact rings** | `translate` to each finger 1:1 per frame, no easing. Land: scale .6→1, `DUR_PRESS` `Pop`. Lift: scale 1→1.4 and fade, accelerate ≤ 140 ms. A thumb or palm draws a dashed ring at .45. | direct manipulation |
| Secondary | **Glow trail** | Each finger leaves a tapered stroke of its last ~140 ms of positions in the accent role, with alpha decaying linearly in time (a clock, not a curve) and an additive glow from a pre-blurred brush. The head is the finger, 1:1. | direct manipulation + clock |
| Secondary | **Gesture trail** | On 3/4-finger axis lock, a 2 dp track runs from the start centroid along the axis. Its fill follows the centroid 1:1 toward a **notch at the commit ratio**, and the notch pops (`Pop` 120 ms) on crossing. Commit: the head travels on 24 dp and fades, the badge shows ✓ and the label. Cancel: springs back (`Spring`, `DUR`) and fades. | research §7.1 |
| Ambient | **Afterglow** | Where a finger lifts, a soft bloom fades over 520 ms (decelerate, as the link ripple). The texture (grid or dots) brightens 6 % under fingers. Nothing moves while idle. | clock exception |

**Budget (8.3 ms per frame at 120 Hz):**
- All drawing in one `Canvas` with `drawWithCache`.
- Trails from preallocated ring buffers (no allocation per frame).
- At most 5 rings, 5 trails and 4 overlays.
- No `RenderEffect` blur on the hot path: glow is a pre-rendered brush.
- **The send happens before drawing**, on the input thread.
- Reduced motion: trails and afterglow off, rings appear without scale, the gesture track becomes a static fill bar.

**Editor.** The same editor for both kinds (spec §5.6): drag, resize from a corner, look sheet, variant picker with a morph between variants (`DUR_HERO`, shared elements per control id), and a comfort check. Exclusive use applies: the touchpad has one holder per computer ("In use by X — Take over?", toast and Take back), while each device gets its own gamepad.

### Accessibility

- TalkBack labels on every control. Custom actions stand in for every gesture: Quick controls, Take over, Take back, Rejoin.
- `liveRegion = Polite` for link words, clipboard voice and take-over toasts.
- Font scale goes through the Size model only.
- Contrast is checked across three base16 schemes in a unit test (`ContrastTest` on the role mixer).
- `minimumInteractiveComponentSize` is at least 44 dp on the glass.
- Reduced motion is honoured: system scale 0, or the user's level.

### Action feedback (the pending-action primitive, native)

- `ui/common/pending/PendingAction.kt` is a state machine:
  - **pressed** under 100 ms, with the 0.96 press scale
  - **working** by 300 ms, with in-place progress
  - **taking long** past the action's threshold: "Still waiting for <computer>…" with Cancel and Try again
  - **outcome**: quiet success, or a plain reason with Try again
- Its wire half is `request.rs`. Every core command returns an `OpId`, and the core sends the additive `ack` where the message has one (Session*, HostGet/HostDo, ConfigSet, ClipboardSet).
- It emits `CoreEvent::Answer{op, outcome}`, or `NoAnswer` after the action's deadline.
- **No frozen button anywhere.**

### WebView defects the native app must not repeat (each has a test)

| # | Defect | Rule | Test (phase) |
|---|---|---|---|
| 1 | The IME's insets squeezed the console | The stage never consumes `WindowInsets.ime`. Only the Keys face pads for it, and the IME opens only on Type/⌨. | `ConsoleImeTest`: with the IME up, the stage bounds are unchanged (P6) |
| 2 | Default video poster | `SurfaceViewRenderer` on Stage Black, no placeholder; revealed on its first-frame callback | `StageFirstFrameTest`: stage pixels black before the first frame (P6) |
| 3 | Connecting state as a floating card | Connecting and no-picture states live **in the stage**, with Leave and Retry | `StageStatesTest` (P6) |
| 4 | Hints over controls | Hints get their own space (a hint row or the coach), never an overlay on a control | `HintOverlapTest`: no hint bounds intersect a clickable node's bounds (P3, P6) |
| 5 | Session trap | Enter on the user's pick only; Back always leaves; a late start is answered `session_detach` | core `late_start_is_detached`; `SessionBackTest` (P6) |
| 6 | Route lost on resume | Saved back stack | `NavRestoreTest` with `ActivityScenario.recreate()` (P1) |
| 7 | No ICE on the phone's hotspot | Gather on every interface (A3) | Hardware check, logged in `plan/memory` (P3) |

## A8. Performance budgets (measured, never asserted)

Measured with Macrobenchmark (startup and frame timing, Baseline Profile on), libwebrtc `getStats`, and the touchpad research's M7 camera method. "Today" numbers are taken first, in P1 Task 17, and P6.

| Metric | Budget | How |
|---|---|---|
| Cold start → home's first frame, cards from cache | p50 ≤ 450 ms on a Pixel 7-class phone; ≤ 900 ms on a 2019 mid-range | `StartupTimingMetric` `timeToInitialDisplay` |
| Cold start → live link words on a card | p50 ≤ 1.5 s via the relay on Wi-Fi | core `Link{Open}` timestamp minus process start, logged |
| Tap card → Computer page | Transition starts within one frame; 0 janky frames | `FrameTimingMetric`: `frameOverrunMs` p95 ≤ 0 over 20 runs |
| Scroll home or Computer page at 120 Hz | `frameDurationCpuMs` p95 ≤ 6 ms | `FrameTimingMetric` |
| Input: event → `DataChannel.send` | p90 ≤ 2 ms (same device clock: `MotionEvent.eventTime` vs `SystemClock.uptimeMillis` at send) | a dev log on the input thread |
| Video glass-to-glass | p50 ≤ the WebView's p50 on the same phone, network and session (hypothesis: about 1 frame better without the WebView compositor) | 240 fps camera, 20 samples each, LAN and relayed |
| Decode | `totalDecodeTime/framesDecoded` p50 ≤ 8 ms | `getStats` |
| Jitter buffer | `jitterBufferDelay/jitterBufferEmittedCount` p50 ≤ 15 ms on LAN | `getStats` |
| APK (arm64-v8a split, R8 on) | ≤ 28 MB after P6; ≤ 25 MB after P8. Today the debug APK is 27.4 MB, with a 2.6 MB wasm and unstripped .so files. | `apkanalyzer` in `scripts/android-build.sh` output |

Browser-reported and daemon-reported numbers are never added together (`CLAUDE.md` hazard).

## A9. Decisions resolved here (each needs a Decision Log line when executed)

1. **Media stack:** libwebrtc via `io.getstream:stream-webrtc-android`. Apache-2.0 + BSD-3 → AGPL-compatible. Reasons in A3.
2. **Core split:** `client-core` (pure Rust, shared with desktop) and `mobile-core` (UniFFI façade). Desktop links `client-core` directly.
3. **Crypto crate:** `ring` 0.17, the daemon's own. One implementation of each primitive on both ends.
4. **Pairing stays on the seat join, for the ceremony only,** then close `4001`. The daemon refuses pairing on control rooms by design. Every feature uses control or view rooms.
5. **Re-pair once.** The WebView key is non-extractable. The app's core is a new device per computer.
   - Until P8 the phone shows up twice on the computer (the app, and its web part), which parallel use makes harmless.
   - P1's home marks web-known computers "Pair the app".
   - **Rejected:** letting a trusted device add a second key to its own trust line. That is a trust-model change for a transitional problem.
6. **Passkeys:**
   - The native ceremony keeps the origin `https://appassets.androidplatform.net` and rpId `appassets.androidplatform.net`, which the daemon already accepts for the app.
   - A passkey is enrolled per device identity, so the new identity enrolls once (P2).
   - `Authenticator.kt` is unchanged.
7. **Terminal:** Termux modules, vendored with a `RemoteTerminalSession` patch.
8. **Images:** Coil 3 + Telephoto (both Apache-2.0).
9. **Navigation:** Navigation 3 with a saveable back stack and predictive back. If its shared-element or predictive-back support is not stable when P1 starts, use type-safe Navigation-Compose 2.9, and say so in the Decision Log.
10. **Size:** local-first, mirrored to computers best-effort.
11. **Logos:** monochrome, base16-tinted, from a pinned Simple Icons release, with a licence-checking script.
12. **Events:** pull-based (`suspend next_event`). The core never calls into the JVM from its own threads.
13. **One layout model for the gamepad and the touchpad** (`wado_protocol::layout`, `kind` defaulting to `gamepad`), with six touchpad variants. Edge and wheel scrolling synthesise two-finger contacts so that libinput's own scroll applies; a daemon `Wheel` is the measured fallback. Precision's Boost goes through Hyprland's sensitivity and never scales on the phone. Laptop's click bar sends `BTN_LEFT`/`BTN_RIGHT`. (HANDOVER 4b.)

## A10. Parity checklist (web client feature → native phase)

| Web feature (files) | Phase |
|---|---|
| Relay link, seat, reconnect, backoff (`relay.js`, `relay_link.js`, `seat.js`, `reconnect*`, `netwatch.js`, `lifecycle.js`, `watchdog.js`) | P1 (control room), P6 (view room) |
| E2E keys, handshake, seal (`e2e_keys.js`, `e2e_handshake.js`, `e2e_seal.js`) | P1 |
| Pairing: QR, typed code, check words (`pair.js`, `landing/pair.rs`, `storage.js`) | P1 |
| Computer list, profiles, rename, forget (`profile.rs`, `persist.rs`, `pages/computers.rs`) | P1 (list, forget), P2 (rename in Connection & trust) |
| Online and link words, link state, health, hops (`online.js`, `link_state.js`, `health.js`, `computer/hops.rs`, `linkmeter.rs`) | P1 (words), P2 (hero hops and meter) |
| Light links / side links for widgets (`side_link.js`) | P1: every computer has a control room |
| Theme, schemes, Appearance, Size (`theme.js`, `theme.rs`, `pages/appearance.rs`, `pages/size.rs`, `ui_size.js`) | P1 |
| Diagnostics, logs, about (`pages/diagnostics.rs`, `logs.js`, `pages/about.rs`) | P1 (core), P7 (link test) |
| Computer page: hero, actions, now playing, quick, settings list (`ui/computer/*`) | P2 |
| Host state and controls: volume, brightness, media, keep awake, Wi-Fi, BT (`host.js`, `ui/host.rs`, `cc/sound.rs`, `pages/network.rs`, `wakelock.js`) | P2 |
| Display & session, Sound & devices, Connection & trust (`computer/pages/*`, `pages/display.rs`, `pages/host.rs`, `pages/connection.rs`, `config.js`, `cfg.rs`) | P2 |
| Passkey card, approve on computer, errors, step-up (`passkey.js`, `passkey_*.js`, `ui/passkey/*`, `stepup.rs`) | P2 |
| Sessions list, join, start, end, take over (`sessions.js`, `computer/sessions.rs`, `session_row.rs`, `home/sessions.rs`, `multi.js`, `multi.rs`) | P2 (list and verbs), P6 (join into the stage) |
| Action feedback, pending (`pending.js`, `pending.rs`, `ask_clock.js`) | P1 (primitive), then each phase's actions |
| Virtual input hub, touchpad, keyboard, gamepad (`computer/input/*`, `pad_*.js`, `deck_*.js`, `gpad/*`, `gamepad.js`, `kb_prefs.js`, `osk.js`) | P3 |
| Pad link (`pad_link.js`, `pad_touch.js`, `pad_pointer.js`) | P3 (already native in part: `PadPacer`) |
| Touchpad feel: gestures, hints, coach, rail, why-cards (`pad_gestures/hints/coach/rail/why/confirm/orient.js`) | P3 |
| Gamepad layouts, editor, picker, comfort check (`gpad/*`) | P3, on the shared layout model, which also carries the **new** touchpad variants (no web counterpart yet) |
| Files manager: list, ops, pins, recents, find, transfers, share-to-computer (`files_*.js`, `targets.js`) | P4 |
| Viewer: every kind in `viewer/kinds.js`, details sheet, menu | P4 |
| Shells: list, open, pty, per device (`pty.js`, xterm.js, `shells` messages, `home/widgets/shell.rs`) | P5 |
| Session: video, audio, stage, console, cockpit, dock, workspaces, window strip, control centre, tiles, quick rail (`video.js`, `audio.js`, `webrtc.js`, `stage_*.js`, `ui/surround/*`, `ui/dock.rs`, `ui/cc/*`, `drawer/*`, `chrome.js`, `bar.js`, `cbar.js`) | P6 |
| Session input: pointer, touch, scroll, tap, keyboard, IME, lock, accel, coalescing, units (`input_*.js`, `gestures.js`, `viewport.js`) | P6 |
| Autorate, stats, latency (`autorate.js`, `stats.js`, `latency.js`) | P6 |
| Apps launcher, menu sheet, actions (`apps.js`, `menu_sheet.js`, `actions.js`, `control.js`) | P6 |
| Presence and ghost pointers, people chip (`presence.js`, `multi.js`) | P7 |
| Clipboard: history, surface, chip, share both ways (`clipboard.js`, `cliphost.js`, `clip_share.js`, `ui/clipboard/*`) | P7 (`ClipSync.kt` stays as the phone side) |
| Push and notifications (`push.js`, `notify.js`, `native_push.js`) | P7 |
| Claude card, picker, Allow/Deny (`claude.js`, `ui/claude/*`) | P7 |
| Lane hints, Retry direct (`lane_hint.js`, `ui/lane_hint.rs`, `lane.rs`) | P7 |
| Link test / speed test (`linktest.js`) | P7 |
| Widgets: media, volume, brightness, clipboard, virtual input, shell, Claude, links, edit mode (`home/widgets/*`, `widget*.js`) | P7 |
| Deep links and shortcuts (`deeplink.js`, `deeplink.rs`; `links/DeepLinks.kt`) | P1 (routes), extended per phase |
| Share sheet "Send to computer" (`native_share.js`; `share/`) | P4 (files), P7 (links and text) |
| Install hint, service worker, signed updates (`install_hint.js`, `app_sw.js`) | Not applicable in the app (P8 deletes) |

---

# Part B — Plan

## Global Constraints

- AGPL-3.0-only on every new crate and module (`license = "AGPL-3.0-only"`). Vendored code keeps its own licence file.
- **Invariants 1, 2, 4 and 5** (`CLAUDE.md`):
  - Input sends happen on a dedicated thread, never behind video or the tokio queue.
  - Video only over WebRTC/UDP.
  - H.264 first in codec preferences.
  - The software-encode notice is shown when the daemon reports it.
- **The relay handshake is frozen.** The core speaks `wado_protocol::relay_wire` as is. Daemon additions are additive `#[serde(default)]` fields.
- **The compositor/server boundary holds.** No phase reaches into `crates/compositor`.
- **No seat-gated feature.** Everything rides a control or view room. The seat is used only for the pairing ceremony (A9.4).
- One module, one job. Kotlin files ≤ about 300 lines. Split on a second job.
- Every colour from a base16 role. Stage Black `#000` only behind video. Two curves only (`Motion.Spring`, `Motion.Pop`) plus the accelerate fade ≤ 180 ms. Duration tokens from DESIGN.md.
- Every touch target ≥ 44 dp on the glass. Text inputs ≥ 16 sp on the glass.
- Plain words first; technical names one tap away (Diagnostics).
- No AI attribution in commits. Short commit messages. `cargo fmt --all --check` clean.
- Builds:
  - Rust host tests: `cargo test -p wado-client-core -p wado-mobile-core`.
  - APK: `CARGO_TARGET_DIR=$PWD/target-android nix develop .#android -c scripts/android-build.sh --debug`.
  - Emulator: `nix develop .#android-emulator -c scripts/android-emulator.sh`.
  - GPU and emulator work needs the Bash sandbox off.
- The daemon in any e2e test is a **release** build.

## Review Focus

1. **A counter gap after a reconnect.** A sealed frame queued before a drop must never be sealed with the old keys or counter. Expect: the outbox is cleared of sealed state on every new handshake, and plaintext frames marked "resend on reconnect" are re-sealed. Test: `outbox_reseals_after_rekey` (Task 7).
2. **A pin mismatch** (a relay standing in for the computer). Expect: the link refuses, the card says "This isn't the computer you paired with — re-pair", and there is no retry storm. Test: `pin_mismatch_refuses_and_stops` (Task 3), `refused_link_does_not_redial` (Task 7).
3. **The app paired with no pairings in the core, but the WebView has some.** Expect: legacy cards show with "Pair the app", and nothing is lost or duplicated. Test: `CoreStateTest.legacy_and_core_merge_by_remote_id` (Task 12).
4. **Process death mid-pairing.** Expect: nothing half-saved. A profile is written only after `e2e_ok`, and the screen restores to Pair with a fresh start. Test: `profile_written_only_after_ok` (Task 8), `NavRestoreTest` (Task 13).
5. **A relay that lacks `control`/`room_strict`** (an older relay). Expect: no seat is taken. The card says "This computer's relay is too old for the app — update it" and offers the web part. Test: `old_relay_never_takes_a_seat` (Task 7).

---

## P1 — Client core + native home with live status + On this phone

**Ships:**
- Pair a computer natively (QR or typed code with check words).
- Home cards with OS logo, honest live link words and session count, over the core's control rooms.
- On this phone: Appearance with Size, Diagnostics, and the existing background switches.
- Navigation restored on resume.

**Still in the WebView:** the Computer page, every door except the native touchpad, settings of a computer, sessions, files, shells, clipboard history, push, Claude and widgets. Doors on a core-paired computer open the WebView at that computer when the web part knows it. Otherwise the door says "Open this once in the web part to pair it there too" (A9.5).

**Risks:**
- UniFFI async in Kotlin (needs kotlinx-coroutines; verify library-mode bindgen emits suspend functions).
- rustls on Android: webpki roots are compiled in, so no platform verifier is needed.
- Navigation 3 maturity (fallback in A9.9).
- Re-pair friction (one code per computer).

### File structure (P1)

```
crates/client-core/Cargo.toml                      new crate, AGPL, deps per Tech Stack, dev-deps: wado (server), tokio test-util
crates/client-core/src/{lib,secret,identity,pins,profiles,os_logo,request,core,event}.rs
crates/client-core/src/e2e/{mod,keys,seal,handshake}.rs
crates/client-core/src/pair/{mod,link,flow}.rs
crates/client-core/src/relay/{mod,url,transport,frames}.rs
crates/client-core/src/link/{mod,state,backoff,outbox,driver}.rs
crates/client-core/src/feeds/{mod,host,sessions}.rs
crates/client-core/tests/{handshake_vs_daemon,live}.rs
crates/mobile-core/src/ffi/{mod,core,secret}.rs    UniFFI façade; Cargo.toml gains wado-client-core
Cargo.toml                                          workspace member crates/client-core
crates/protocol/src/host.rs                         SysInfo.os_like (additive)
crates/server/src/platform/linux/sysinfo.rs         read ID_LIKE
crates/server/src/control/policy.rs                 SessionsRequest → Need::Trusted
scripts/os-logos.mjs                                logos from a pinned Simple Icons release
scripts/core-e2e.mjs                                relay + release daemon + mint → cargo test --test live
android/app/build.gradle.kts, gradle/libs.versions.toml  navigation3, kotlinx-coroutines, compose-ui-test, macrobenchmark module
android/app/src/main/java/io/github/sandptel/wado/
  core/CoreHost.kt             create WadoCore, pump next_event into CoreState
  core/CoreState.kt            pure reducer: CoreEvent → HomeModel
  core/KeystoreSecretBox.kt    SecretBox over an AndroidKeyStore AES-GCM key
  nav/Routes.kt                @Serializable route keys
  nav/AppNav.kt                back stack, transitions, predictive back
  ui/home/HomeScreen.kt        (modify) sections: Computers, Running, On this phone
  ui/home/ComputerCard.kt      (rewrite) logo disc, link words, sessions, ▶
  ui/home/OsLogo.kt            slug → drawable, disc
  ui/home/LinkWords.kt         dot + words; ripple / breathe / still
  ui/home/OnThisPhone.kt       nav rows
  ui/pair/PairScreen.kt        (modify) scan → core.pair
  ui/pair/TypedCode.kt         relay + Remote ID + XXXX-XXXX form
  ui/pair/CheckWords.kt        four words, "Confirm on the computer"
  ui/phone/AppearanceScreen.kt
  ui/phone/SizeModel.kt        SizeScope, LocalSize, Modifier.tap()
  ui/phone/DiagnosticsScreen.kt
  ui/phone/BackgroundScreen.kt (moved from settings/SettingsScreen.kt)
  ui/common/pending/PendingAction.kt
  ui/theme/Motion.kt           (modify) tokens + motion level
  prefs/Prefs.kt               (modify) size, motion, scheme, accent, radius keys
android/app/src/main/res/drawable/os_*.xml        generated
android/app/src/test/java/io/github/sandptel/wado/{CoreStateTest,SizeModelTest,OsLogoTest,PendingActionTest,ContrastTest}.kt
android/app/src/androidTest/java/io/github/sandptel/wado/{HomeUiTest,NavRestoreTest,PairUiTest}.kt
android/benchmark/                                  Macrobenchmark: startup, home→scroll
```

### Task 1: client-core crate and the E2E key schedule

**Files:**
- Create: `crates/client-core/Cargo.toml`, `crates/client-core/src/lib.rs`, `crates/client-core/src/e2e/mod.rs`, `crates/client-core/src/e2e/keys.rs`
- Modify: `Cargo.toml` (workspace members)

**Interfaces:**
- Produces:
  - `keys::session_keys(shared: &[u8;32], salt: &[u8;32]) -> ([u8;32],[u8;32])` (c2d, d2c)
  - `keys::pair_key(shared, salt) -> [u8;32]`
  - `keys::sas_key(shared, salt) -> [u8;32]`
  - `keys::pair_mac(pair_code: &[u8], h_t2: &[u8;32]) -> [u8;32]`
  - `keys::typed_mac(k_pair: &[u8;32], h_t2: &[u8;32], code8: &[u8;8]) -> [u8;32]`
  - `keys::sas_bytes(k_sas: &[u8;32], h_t2: &[u8;32], nonce: &[u8;32]) -> [u8;4]`
  - Labels come from `wado_protocol::envelope::{LABEL, PAIR_LABEL, SAS_LABEL}`.

- [ ] **Step 1: Write the failing tests.** Expected hex strings are copied from `node scripts/e2e-vector.mjs` output (run it first). They must equal what `crates/server/src/e2e/seal.rs` tests `vector`, `pair_vector` and `sas_vector` pin.

```rust
#[test] fn vector_session_keys() { let (c2d, d2c) = session_keys(&[7;32], &[9;32]); assert_eq!(hex(&c2d), C2D_HEX); assert_eq!(hex(&d2c), D2C_HEX); }
#[test] fn vector_typed_mac() { let k = pair_key(&[7;32], &[9;32]); assert_eq!(hex(&typed_mac(&k, &[5;32], b"K7QF2MXP")), TYPED_HEX); }
#[test] fn vector_sas_bytes() { /* inputs exactly as e2e-vector.mjs's sas section */ assert_eq!(hex(&sas_bytes(&sas_key(&[7;32],&[9;32]), &H_T2, &NONCE)), SAS_HEX); }
```

- [ ] **Step 2: Run** `cargo test -p wado-client-core e2e::keys`. Expected: FAIL (unresolved functions).
- [ ] **Step 3: Implement with `ring::hkdf` and `ring::hmac`.** HKDF-SHA256 with `salt = H(T1)` and info = label, 64 bytes split 32/32 for the session keys.
- [ ] **Step 4: Run** the same command. Expected: PASS.
- [ ] **Step 5: Commit** `client-core: e2e key schedule`.

### Task 2: The seal

**Files:** Create `crates/client-core/src/e2e/seal.rs`

**Interfaces:**
- Produces:
  - `Sealer::new(key:[u8;32])`
  - `Sealer::seal(&mut self, plaintext:&[u8]) -> String`, which returns the whole frame `{"type":"sealed","n":<u64>,"c":"<b64url>"}`
  - `Opener::new(key)`
  - `Opener::open(&mut self, frame:&str) -> Result<Vec<u8>, SealError>`
  - `SealError::{Malformed, OutOfOrder{want,got}, Auth}`
- Nonce = 4 zero bytes ‖ counter as u64 BE (`wado_protocol::envelope::nonce`).

- [ ] **Step 1: Failing tests:**
  - `vector_seal_session_stop`: with c2d from Task 1 and counter 0, `{"type":"session_stop"}` → the ciphertext hex from `e2e-vector.mjs`.
  - `opener_rejects_replay`: opening n=0 twice → `OutOfOrder{want:1,got:0}`.
  - `opener_rejects_gap`: n=2 first → `OutOfOrder`.
  - `tamper_is_auth_error`.
  - `roundtrip_with_daemon_opener`: the frame opens with `wado::e2e::seal::Opener` (dev-dependency).
- [ ] **Step 2: Run** `cargo test -p wado-client-core e2e::seal`. Expected: FAIL.
- [ ] **Step 3: Implement** with `ring::aead::LessSafeKey` AES_256_GCM and base64 URL_SAFE_NO_PAD.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `client-core: sealed frames`.

### Task 3: Client handshake, identity, pins and SecretBox

**Files:** Create `crates/client-core/src/{secret,identity,pins}.rs`, `crates/client-core/src/e2e/handshake.rs`, `crates/client-core/tests/handshake_vs_daemon.rs`

**Interfaces:**
- Produces:
  - `trait SecretBox: Send + Sync { fn wrap(&self, plain: Vec<u8>) -> Result<Vec<u8>, String>; fn unwrap(&self, sealed: Vec<u8>) -> Result<Vec<u8>, String>; }`
  - `Identity::load_or_create(dir:&Path, sb:&dyn SecretBox) -> Result<Identity>`, with fields `client_key: String` (random 16 bytes, b64url) and `pub_raw: [u8;32]`, and `fn sign(&self, msg:&[u8]) -> [u8;64]`. The seed is stored only wrapped, in `identity.bin`.
  - `Pins::load(dir)`, `Pins::get(remote_id) -> Option<Pin{pin:String, from:PinFrom}>`, `Pins::set(remote_id, pin, from)`. `PinFrom::{Qr, FirstUse}`. The Remote ID is normalised with `relay_wire::normalize_remote_id`.
  - `ClientHandshake::start(room_id, client_key, identity) -> (Self, String /*e2e_hello json*/)`
  - `ClientHandshake::on_reply(&mut self, reply_json:&str, pin: Option<&Pin>, proof: Proof) -> Result<(String /*e2e_finish*/, Keys), HsError>`
  - `Proof::{None, Qr(code), Typed(code8)}`
  - `Keys{ c2d, d2c, sas: Option<[u8;4]> }`
  - `HsError::{PinMismatch, BadSignature, Version, Malformed, Failed(String)}`
- T1 and T2 exactly per `envelope.rs`. `host_pk` pin = `wado::e2e::host_key::pin_of` format (SHA-256 of the public key, b64url).

- [ ] **Step 1: Failing tests** (in `tests/handshake_vs_daemon.rs`, against the daemon's own `wado::e2e::handshake::{reply, finish}` and `HostKey` in a temp dir):
  - `qr_pairing_completes_with_daemon`: `finish` returns `Done` with the QR proof; the client keys equal the daemon's keys.
  - `typed_code_yields_sas_words_equal_on_both_ends`: client SAS words == `wado_protocol::envelope::sas_words` of the daemon's SAS.
  - `pin_mismatch_refuses_and_stops`: a pin for another host key → `Err(PinMismatch)` and no finish frame.
  - `bad_host_signature_refuses`.
  - `first_use_returns_pin_to_store`.
  - Plus `identity_is_stable_across_loads`, and `identity_seed_never_written_unwrapped`, which scans the dir for the raw seed bytes.
- [ ] **Step 2: Run** `cargo test -p wado-client-core --test handshake_vs_daemon`. Expected: FAIL.
- [ ] **Step 3: Implement.** Use `ring::agreement` X25519 ephemerals and `ring::signature` Ed25519 (`Ed25519KeyPair::from_seed_unchecked`). Verify the reply's `sig` over H(T1) with `host_pk` **before** sending anything else (the step the test `Client` in `server/src/e2e/handshake.rs:178` skips).
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `client-core: client handshake, identity, pins`.

### Task 4: Profiles and the OS logo map

**Files:**
- Create: `crates/client-core/src/{profiles,os_logo}.rs`
- Modify: `crates/protocol/src/host.rs` (`SysInfo.os_like: Vec<String>`, `#[serde(default)]`), `crates/server/src/platform/linux/sysinfo.rs` (parse `ID_LIKE`, space-separated)

**Interfaces:**
- Produces:
  - `Profile{ key:String, name:String, relay_url:String, remote_id:String, os_id:String, os_like:Vec<String>, desktop:String, hue:u8, last_used_ms:u64 }`
  - `Profiles::load(dir) -> Profiles`, `list() -> Vec<Profile>`, `upsert(Profile)`, `forget(key)`, `save()`. `save` is atomic: write `profiles.json.tmp`, fsync, rename.
  - `os_logo::slug(os_id:&str, os_like:&[String]) -> &'static str`

- [ ] **Step 1: Failing tests:**
  - `exact_id_wins` (`"pop"` → `"popos"`)
  - `id_like_order_is_followed` (`os_id "cachyos"`, like `["arch"]` → `"archlinux"`)
  - `opensuse_family` (`"opensuse-tumbleweed"` → `"opensuse"`)
  - `unknown_falls_back_to_linux`
  - `every_slug_has_a_drawable`: reads `android/app/src/main/res/drawable/os_<slug>.xml` existence for every value `slug` can return; ignored until Task 15 lands, then un-ignored there
  - `save_is_atomic_and_roundtrips`
  - `old_file_without_os_fields_loads`
  - In `wado-protocol`: `sysinfo_without_os_like_parses`
  - In `wado`: `id_like_is_read`
- [ ] **Step 2: Run** `cargo test -p wado-client-core profiles os_logo && cargo test -p wado-protocol sysinfo && cargo test -p wado sysinfo`. Expected: FAIL.
- [ ] **Step 3: Implement.** The mapping table is A7's list.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `client-core: profiles, OS logo map; daemon: os_like`.

### Task 5: Pairing links and relay URLs

**Files:** Create `crates/client-core/src/pair/{mod,link}.rs`, `crates/client-core/src/relay/{mod,url}.rs`

**Interfaces:**
- Produces:
  - `pair::link::parse(link:&str) -> Result<PairLink{relay_url, remote_id, code: PairCode, hk: Option<String>}, LinkError>`. It accepts `https://…/?relay&id&pair&hk`, `wado://pair?…`, and the web client's `?go=` forms, which are ignored.
  - `PairCode::{Qr(String), Typed([u8;8])}`
  - `pair::link::typed(input:&str) -> Option<[u8;8]>`: strips `-` and spaces, uppercases, and accepts exactly 8 of the alphabet `wado pair` uses.
  - `relay::url::seat_pair(relay, rid, client_key, name) -> String` (`/join/<rid>?client=&name=&pair=1`)
  - `relay::url::control(relay, rid, client_key, name) -> String` (`…&control=1&strict=1`)
  - `relay::url::view(...)` (P6)
  - `http(s)` maps to `ws(s)`. Query values are percent-encoded.

- [ ] **Step 1: Failing tests:**
  - `parses_qr_link_from_wado_qr`: the exact output shape of `wado qr --relay http://r.example --id 123456789`, as in `scripts/multi-e2e.mjs mint()`
  - `parses_wado_scheme`
  - `typed_code_normalises_dash_and_case`
  - `typed_code_rejects_ambiguous_letters`: check the alphabet in `crates/server/src/gate/pairing.rs` first
  - `control_url_has_control_and_strict`
  - `seat_pair_url_matches_relay_link_js`: the same params as `relay_link.js:241`
  - `names_are_percent_encoded`
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `client-core: pairing links, relay URLs`.

### Task 6: Transport and frame classification

**Files:** Create `crates/client-core/src/relay/{transport,frames}.rs`

**Interfaces:**
- Produces:
  - `#[async_trait] trait Transport { async fn send(&mut self, text:String) -> io::Result<()>; async fn recv(&mut self) -> Option<io::Result<String>>; async fn close(&mut self, code:u16); }`
  - `WsTransport::connect(url:&str) -> io::Result<WsTransport>`: tokio-tungstenite 0.24 with `rustls-tls-webpki-roots`, as `crates/server/Cargo.toml` uses it.
  - `MemTransport::pair() -> (MemTransport, MemTransport)` for tests.
  - `frames::classify(text:&str) -> Frame::{Wire(WireMsg), E2e(E2eMsg), Sealed, Other}`
  - `LEAVE` = `relay_wire::LEAVE_CLOSE_CODE`

- [ ] **Step 1: Failing tests:**
  - `join_accepted_is_wire`
  - `e2e_reply_is_e2e`
  - `sealed_is_sealed`
  - `unknown_type_is_other_not_error` (forward compatibility)
  - `mem_transport_roundtrip`
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `client-core: relay transport`.

### Task 7: Link state machine, backoff and outbox

**Files:** Create `crates/client-core/src/link/{mod,state,backoff,outbox,driver}.rs`, `crates/client-core/src/request.rs`

**Interfaces:**
- Consumes: Tasks 2, 3, 5 and 6.
- Produces:
  - `LinkState::{Idle, Dialing, Securing, Open{since_ms}, Reconnecting{attempt:u32, in_ms:u64}, Offline{seen_ms:Option<u64>}, Refused{why:Refusal}, RelayTooOld}`
  - `Refusal::{PinMismatch, NotTrusted, Revoked}`
  - `state::step(&mut Link, Input) -> Vec<Action>`, which is pure. Inputs are transport events, timers and commands. Actions are send, close, arm a timer, emit a state, or emit a message.
  - `backoff::delay(attempt:u32, jitter:f64) -> Duration`: 250 ms, 500 ms, 1 s, 2 s, 4 s, then capped at 8 s, ±20 % jitter.
  - `Outbox::push(msg: RelayMsg, class: Class)`, `Outbox::next(&mut self, now) -> Option<RelayMsg>` (pacing with `wado_protocol::lane::Pace`, 56 KiB/s, 1.25 MiB burst), `Outbox::rekey(&mut self)`.
  - `Class::{Urgent, Bulk, Spare}`: the mapping mirrors `crates/server/src/lane/classify.rs`.
  - `driver::run(link_cfg, transport_factory, events: mpsc::Sender<CoreEvent>, cmds: mpsc::Receiver<LinkCmd>)`
  - `request::Ops::next_id() -> u64`, `Ops::deadline(op, ms)`, and `Ops::answer(op, Outcome)`, with `Outcome::{Done, Failed(String), NoAnswer}`.

- [ ] **Step 1: Failing tests:**
  - `old_relay_never_takes_a_seat`: `JoinAccepted.caps` without `control` → `RelayTooOld`, close `4001`, no frames sent after.
  - `refused_link_does_not_redial`
  - `join_denied_offline_is_offline_with_backoff`
  - `backoff_is_capped_and_jittered`
  - `urgent_jumps_bulk`
  - `outbox_seals_at_dequeue`: counters are contiguous even when Urgent jumps.
  - `outbox_reseals_after_rekey`
  - `pace_holds_budget`: 2 MiB pushed, at most burst + rate×t sent.
  - `no_answer_after_deadline`
- [ ] **Step 2: Run** `cargo test -p wado-client-core link request`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - The state machine is pure. `driver.rs` alone does I/O.
  - Ping: answer the relay's `ping` cap as `relay_link.js` does.
  - Keep `Open.since_ms` for the link words.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `client-core: link state, outbox, requests`.

### Task 8: Pairing flow

**Files:** Create `crates/client-core/src/pair/flow.rs`

**Interfaces:**
- Consumes: Tasks 3, 4, 5, 6 and 7 (`Profiles`, `Pins`, `Identity`).
- Produces: `pair::flow::run(link: PairLink, name:String, ctx) -> impl Stream<Item=PairStep>`, with
  - `PairStep::{Dialing, Securing, CheckWords{words:[String;4]}, WaitingForComputer, Done{key:String}, Failed{why:String}}`
- Rules:
  - Seat join with `pair=1`.
  - On `e2e_ok`: write the pin (QR `hk` → `from: Qr`), upsert the profile, close `4001`.
  - Typed codes wait at most 2 min for the computer's yes. On `e2e_fail` the reason is passed through in plain words.
  - `JoinDenied` because another device is using the seat → `Failed{"<computer> is busy with another device's desktop. Try again in a moment, or pair from the computer."}`.

- [ ] **Step 1: Failing tests** (with `MemTransport` and an in-process daemon from `wado::e2e::handshake`):
  - `qr_flow_saves_profile_and_pin_then_leaves`
  - `typed_flow_shows_words_then_done_on_yes`
  - `typed_flow_fails_plainly_on_no`
  - `profile_written_only_after_ok`
  - `busy_seat_is_plain_failure`
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `client-core: pairing flow`.

### Task 9: Feeds (host, sessions) and the daemon's control-room sessions rule

**Files:**
- Create: `crates/client-core/src/feeds/{mod,host,sessions}.rs`
- Modify: `crates/server/src/control/policy.rs` (`RelayMsg::SessionsRequest` → `Need::Trusted`, read-only, per the parallel-use table: "sessions list: shared"), plus its test list.

**Interfaces:**
- Produces:
  - `HostView{ name, os, logo:&'static str, desktop, uptime_s, caps:Vec<String>, playing: Option<NowPlaying>, volume: Option<f32>, brightness: Option<f32>, awake: Option<bool> }`, from `HostState`. `playing`, `volume`, `brightness` and `awake` are read now and used in P2.
  - `SessionRow{ instance, apps_first:String, apps_more:u32, lead:String, participants:u32 }`
  - `feeds::host::poll_ms(visible:bool) -> u64`: 5000 when visible, otherwise none (no polling off screen).

- [ ] **Step 1: Failing tests:**
  - `host_state_maps_os_and_logo`
  - `missing_fields_are_none_not_zero` (honest status)
  - `sessions_summary_maps_rows`
  - In `wado`: `control::policy` test `sessions_list_is_trusted_on_control`, while `session_launch` stays `NotControl`
- [ ] **Step 2: Run** `cargo test -p wado-client-core feeds && cargo test -p wado control::policy`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `client-core: host and sessions feeds; daemon: sessions list on control rooms`.

### Task 10: Core façade and events

**Files:** Create `crates/client-core/src/{core,event}.rs`

**Interfaces:**
- Produces: `Core::new(dir:PathBuf, sb:Arc<dyn SecretBox>, device_name:String) -> Result<Core>`, which owns a 2-worker tokio runtime. Methods:
  - `computers() -> Vec<ComputerView>`
  - `watch(keys:Vec<String>)`: hold control rooms for exactly these, drop the rest
  - `pair(link:String) -> u64`
  - `pair_typed(relay:String, rid:String, code:String) -> u64`
  - `forget(key) -> u64`
  - `async next_event() -> CoreEvent`
  - `shutdown()`
- `CoreEvent::{Computers{list:Vec<ComputerView>}, Link{key, state:LinkView}, Host{key, view:HostView}, Sessions{key, rows:Vec<SessionRow>}, Pair{op, step:PairStep}, Answer{op, outcome:Outcome}}`
- `ComputerView{ key, name, os, logo, desktop, hue }`
- `LinkView` = `LinkState` plus `words:String`, the plain words:
  - "Online"
  - "Connecting…"
  - "Reconnecting in 4 s"
  - "Offline · seen 2 h ago"
  - "Can't reach its relay"
  - "Not this computer — re-pair"
  - "Relay too old for the app"
- The event queue is bounded at 256, coalescing `Link`/`Host`/`Sessions` per key, newest wins. `Pair` and `Answer` are never coalesced.

- [ ] **Step 1: Failing tests:**
  - `watch_opens_and_drops_links` (with a transport factory of `MemTransport`s)
  - `events_coalesce_per_key_newest_wins`
  - `pair_and_answer_never_coalesce`
  - `link_words_are_plain`
  - `shutdown_closes_with_leave_code`
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `client-core: Core and events`.

### Task 11: Live e2e against a local relay and a release daemon

**Files:** Create `crates/client-core/tests/live.rs` (`#[ignore]`), `scripts/core-e2e.mjs`

**Interfaces:**
- Consumes: Task 10's `Core`.
- `scripts/core-e2e.mjs`:
  1. Starts `target/release/wado-relay` and a `target/release/wado daemon` against it, with temporary `XDG_CONFIG_HOME`/`XDG_RUNTIME_DIR`. Copy `start()`, `waitFor()`, `cleanup()` and `mint()` from `scripts/multi-e2e.mjs`.
  2. Kills only its own processes, by PID.
  3. Runs `cargo test -p wado-client-core --test live -- --ignored --nocapture` with `WADO_E2E_LINK=<minted link>`.

- [ ] **Step 1: Write `live.rs`:**
  - `pairs_then_watches_and_reads_host`: pair via the link → `PairStep::Done` within 10 s; `watch([key])` → `Link{Open}` within 5 s; `Host` with a non-empty `os` and `logo != ""` within 5 s; the trust file `~/.config/wado/trusted_clients` (temp) has a line with the core's `client_key`.
  - `second_core_cannot_use_spent_code`.
  - `control_room_takes_no_seat`: while the core is watching, a second seat join from a JS device (as `multi-e2e.mjs` makes one) is accepted without takeover.
- [ ] **Step 2: Run** `cargo build --release -p wado -p wado-relay && node scripts/core-e2e.mjs`, with the Bash sandbox off. Expected: FAIL until Tasks 1–10 hold, then `3 passed`.
- [ ] **Step 3: Fix** what the live run shows. Log any daemon or relay mismatch in `plan/memory/shared/` before fixing.
- [ ] **Step 4: Run** again. Expected: PASS.
- [ ] **Step 5: Commit** `client-core: live e2e`.

### Task 12: UniFFI façade and Kotlin core host

**Files:**
- Create: `crates/mobile-core/src/ffi/{mod,core,secret}.rs`, `android/.../core/{CoreHost,CoreState,KeystoreSecretBox}.kt`, `android/app/src/test/.../CoreStateTest.kt`
- Modify: `crates/mobile-core/Cargo.toml` (+ `wado-client-core`, uniffi `tokio` feature), `crates/mobile-core/src/lib.rs`, `android/gradle/libs.versions.toml` (kotlinx-coroutines), `WadoApp.kt` (own `CoreHost`)

**Interfaces:**
- Produces:
  - `#[uniffi::export(with_foreign)] trait SecretBox`
  - `#[derive(uniffi::Object)] struct WadoCore`, mirroring Task 10's methods
  - UniFFI Records and Enums for every event type
  - Kotlin: `CoreHost.events: SharedFlow<CoreEvent>`, `CoreState.reduce(model: HomeModel, ev: CoreEvent): HomeModel` (pure), `HomeModel{ computers: List<CardModel>, running: List<RunningModel>, pairing: PairModel? }`
  - `KeystoreSecretBox`: AES-256-GCM key `wado.core.secret` in AndroidKeyStore, no user-auth requirement (the core must run headless for Stay connected); format IV(12) ‖ ciphertext.

- [ ] **Step 1: Failing tests** (`CoreStateTest`):
  - `link_event_updates_only_that_card`
  - `host_event_sets_logo_and_os_line`
  - `pair_done_adds_card_and_clears_pairing`
  - `legacy_and_core_merge_by_remote_id`: a web-known computer from `Prefs.computers` with no core profile → `CardModel.needsAppPairing = true`; one with both → a single card
- [ ] **Step 2: Run** `cd android && ./gradlew test --tests '*CoreStateTest*'`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - `scripts/android-core.sh` already builds library-mode bindings; check that the generated `suspend fun nextEvent()` exists.
  - `CoreHost` pumps on `Dispatchers.Default` and posts to `StateFlow<HomeModel>`.
- [ ] **Step 4: Run** the unit tests and `scripts/android-build.sh --debug`. Expected: PASS and an APK.
- [ ] **Step 5: Commit** `android: core host over UniFFI`.

### Task 13: Navigation stack and restore

**Files:**
- Create: `android/.../nav/{Routes,AppNav}.kt`, `android/app/src/androidTest/.../NavRestoreTest.kt`
- Modify: `ui/AppRoot.kt` (the WebView stays as one destination, `Route.Web(key, door)`), `ui/Route.kt` (deleted, replaced by `Routes.kt`), `MainActivity.kt` (back handling through the stack)

**Interfaces:**
- Produces:
  - `@Serializable sealed interface Dest { Home; Pair; PairTyped; Phone(page: PhonePage); Web(key:String?, door:String?); Pad(key:String) }`
  - `AppNav(backStack)` with `Motion.Spring` push and pop (480 / 340 ms) and predictive back.

- [ ] **Step 1: Failing test** `NavRestoreTest.route_survives_recreate`: navigate Home → Phone(Appearance), call `scenario.recreate()`, then assert the Appearance title is shown. Also `back_from_web_returns_home`.
- [ ] **Step 2: Run** `nix develop .#android-emulator -c scripts/android-emulator.sh --tests` (Task 17 adds `--tests`; until then run `./gradlew connectedDebugAndroidTest`). Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `android: navigation stack`.

### Task 14: Home — cards with logo, link words, sessions, ▶; legacy cards

**Files:**
- Create: `ui/home/{OsLogo,LinkWords}.kt`, `android/app/src/androidTest/.../HomeUiTest.kt`
- Modify: `ui/home/{HomeScreen,ComputerCard}.kt`, `ui/theme/Motion.kt` (tokens and motion level)

**Interfaces:**
- Consumes: `HomeModel` (Task 12) and `os_*` drawables (Task 15).
- Produces:
  - `ComputerCard(card: CardModel, onOpen, onPlay, onDoor)`: tap opens `Web(key, null)` until P2.
  - `LinkWords(view: LinkViewModel)`
  - `OsLogo(slug, hue)`

- [ ] **Step 1: Failing tests:**
  - `HomeUiTest.card_shows_logo_name_os_and_words`
  - `offline_card_play_is_disabled_with_reason`: content description "Desktop — Linux-box is offline"
  - `legacy_card_offers_pair_the_app`
  - `running_section_names_the_computer`
  - `targets_are_44dp`: every clickable is at least 44 dp
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement** to A7's card spec.
  - Ripple on Direct samples only.
  - Breathe at 2.4 s ease-in-out (ambient exception) for Reconnecting.
  - Still for Offline.
  - The ▶ glows only while a session runs.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `android: native home cards`.

### Task 15: OS logos from Simple Icons

**Files:** Create `scripts/os-logos.mjs`, `android/app/src/main/res/drawable/os_*.xml` (generated, committed), `android/app/src/main/assets/licenses/os-logos.txt`, `android/app/src/test/.../OsLogoTest.kt`

**Interfaces:**
- Produces:
  - one `R.drawable.os_<slug>` per slug in `os_logo.rs`, plus `os_linux`
  - `OsLogo.drawable(slug): Int`

- [ ] **Step 1: Failing tests:**
  - `OsLogoTest.every_core_slug_has_a_drawable`. The slug list is exported by a `mobile-core` function `os_logo_slugs() -> Vec<String>`, added here.
  - The Rust test `every_slug_has_a_drawable` from Task 4 is un-ignored.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement the script.**
  - Pin the `simple-icons` npm version in the script.
  - For each slug, read `icons/<slug>.svg` path data into a 24×24 VectorDrawable with `fillColor="@android:color/white"` (tinted at draw).
  - Exit non-zero if a slug is missing or its JSON entry has a `license` other than CC0.
  - Write `title`, `source` and `guidelines` per slug to `os-logos.txt`.
- [ ] **Step 4: Run** `node scripts/os-logos.mjs && ./gradlew test --tests '*OsLogoTest*' && cargo test -p wado-client-core os_logo`. Expected: PASS.
- [ ] **Step 5: Commit** `android: OS logos (Simple Icons, CC0)`.

### Task 16: Native pairing screens

**Files:**
- Create: `ui/pair/{TypedCode,CheckWords}.kt`, `android/app/src/androidTest/.../PairUiTest.kt`
- Modify: `ui/pair/PairScreen.kt` (a scanned link goes to `core.pair(link)` instead of the WebView), `links/DeepLinks.kt` (`wado://pair` → `Dest.Pair`)

**Interfaces:**
- Consumes: `PairModel` from `HomeModel.pairing`.
- Produces: the screens, wired through `PendingAction` (Task 17).

- [ ] **Step 1: Failing tests:**
  - `PairUiTest.typed_code_shows_four_words_and_waits`
  - `failure_reason_is_plain_with_try_again`
  - `done_returns_home_with_new_card_arriving`
  - `code_field_text_is_16sp_on_glass`
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
  - The words are in mono, large, with "Check these match the computer's screen, then say yes there."
  - A 2 min countdown as a linear clock.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit** `android: native pairing`.

### Task 17: On this phone — Appearance, Size, Diagnostics, Background; PendingAction; emulator tests

**Files:**
- Create: `ui/phone/{AppearanceScreen,SizeModel,DiagnosticsScreen,BackgroundScreen}.kt`, `ui/home/OnThisPhone.kt`, `ui/common/pending/PendingAction.kt`, the tests `SizeModelTest.kt`, `PendingActionTest.kt`, `ContrastTest.kt`, and `android/benchmark/` (Macrobenchmark module: `StartupBenchmark`, `HomeScrollBenchmark`)
- Modify:
  - `prefs/Prefs.kt`
  - `ui/settings/SettingsScreen.kt` (deleted; content moved to `BackgroundScreen.kt`)
  - `android/app/build.gradle.kts`: copy `crates/client/src/schemes.json` into assets the way `LicenceAssets` copies the licence
  - `scripts/android-emulator.sh`: `--tests` runs `connectedDebugAndroidTest`

**Interfaces:**
- Produces:
  - `SizeScope(content)`, `LocalSize: Size{text:Float, follow:Boolean, ui:Float}`, `Modifier.tap()`
  - `PendingAction.State::{Idle, Pressed, Working, TakingLong(waitingFor:String), Failed(why:String), Done}`, driven by `start(op)`, `answer(Outcome)` and a clock
  - Thresholds: pressed under 100 ms, working at 300 ms, taking long at the action's own threshold (default 4 s)

- [ ] **Step 1: Failing tests:**
  - `SizeModelTest`:
    - `compact_keeps_44dp_on_glass`
    - `follow_uses_system_scale_only_when_on`
    - `text_range_is_85_to_150_in_5s`
  - `PendingActionTest`:
    - `no_working_state_before_300ms`
    - `taking_long_offers_cancel_and_retry`
    - `no_answer_is_plain_failure`
  - `ContrastTest.roles_meet_aa_on_three_schemes`: `default-dark`, `catppuccin-latte`, and one more from `schemes.json`
- [ ] **Step 2: Run** `./gradlew test`. Expected: FAIL.
- [ ] **Step 3: Implement.**
  - The Appearance sample applies on release.
  - Diagnostics lists per computer: the link state's technical name, the relay caps from `JoinAccepted`, the core version and the last error.
- [ ] **Step 4: Run** the unit tests, `scripts/android-emulator.sh --tests`, and `./gradlew :benchmark:connectedCheck` on the emulator (record numbers only; budgets are judged on a phone). Expected: PASS. Record the startup numbers in `plan/memory/latency/measurements.md` under "Android native P1".
- [ ] **Step 5: Commit** `android: On this phone, Size, pending actions`.

### Task 18: P1 close-out

- [ ] Update `docs/android.md`: the core, the new identity and re-pair, what is still WebView.
- [ ] Add the Decision Log lines for A9.1–A9.12 that P1 executed.
- [ ] Add the `CHANGELOG.md` entry.
- [ ] Run the full gate: `cargo fmt --all --check && cargo test -p wado-client-core -p wado-mobile-core -p wado-protocol && cargo test -p wado control && node scripts/core-e2e.mjs && scripts/android-build.sh --debug && scripts/android-emulator.sh --tests`. Expected: all green.
- [ ] Commit `docs: native Android P1`.

---

## P2 — Computer page, settings, passkey (outline)

**Ships:**
- The native Computer page: hero with logo, meta strip and hops; Desktop / Virtual input / Files / Shell actions, where non-native doors open the WebView at that door; Now playing; Quick.
- The **quick lip** on home cards.
- The Sessions list with verbs.
- The Passkey card.
- Display & session, Sound & devices, Connection & trust (rename, forget, grants, owner-only keys).
- The Apply bar.
- M2/M4 shared element and predictive back.

**Tasks (one module each):**
1. **daemon:** serve `ConfigSet`, `SessionStop`/`SessionTakeover` verbs, `Passkey*` and the host page reads on control rooms (`control/policy.rs` + tests, per parallel use).
2. core `config/view.rs` (typed `ConfigState` views per page)
3. core `config/set.rs` (`ConfigSet` builders, staged vs live)
4. core `host/actions.rs` (`HostDo` builders + acks)
5. core `passkey/{msgs,flow}.rs`: move `webauthn.rs` in from mobile-core. Ceremony timeouts and exits as decided: Try again, Not now, Approve on the computer, owner off-switch.
6. core `feeds/host.rs` live media, volume, brightness and awake at 1 s while the page is visible
7. `ui/computer/ComputerScreen.kt` + `Hero.kt` + `MetaStrip.kt` + `HopBar.kt`
8. `ui/computer/Actions.kt` (Virtual input as a group with three doors)
9. `ui/computer/NowPlaying.kt` (M10, M11, M12)
10. `ui/computer/Quick.kt` (shared with the home lip; `ui/common/ThickSlider.kt` with M8; `ToggleTile.kt` with the Morphing Toggle Rule)
11. `ui/home/QuickLip.kt` (swipe-up or long-press unfold, the volume-keys binding)
12. `ui/computer/Sessions.kt` (Join, Start my own, End, Take over for size)
13. `ui/computer/pages/{DisplaySession,SoundDevices,ConnectionTrust}.kt` + `ApplyBar.kt` + `Eyebrow.kt`
14. `ui/passkey/{PasskeyCard,PasskeySheet}.kt` (`Authenticator.kt` reused, origin per A9.6)
15. `nav` shared-element transitions (M2, M4, M17)

**Tests:**
- Rust: `config_view_*`, `config_set_*`, `passkey_assertion_verifies_with_daemon` (dev-dependency verifier, as `tests/passkey_roundtrip.rs`), `host_do_ack_*`.
- Kotlin: page reducers.
- Emulator: `ComputerPageTest`, `QuickLipTest`, `PasskeyTimeoutTest`, `SharedElementInterruptTest` (a second tap mid-transition reverses).
- e2e: `core-e2e.mjs` adds `config_set_roundtrip` and `host_do_volume_acks`.
- Macrobenchmark: home → Computer page frame timing.

**WebView still covers:** virtual input except the native touchpad, files, shells, the session, clipboard, push, Claude and widgets.

**Risks:**
- The shared-element performance of the hero with a large wash. Measure, and fall back to the crossfade.
- The new passkey enrollment.

## P3 — Virtual input: Touchpad (with variants), Keyboard, Gamepad on one layout model (outline)

P3 is the biggest UI phase. It splits cleanly into **P3a** (transport and the Plain touchpad) and **P3b** (the layout model, variants, editor and gamepad), each shippable.

**P3a ships:**
- libwebrtc in the app, for the pad PC only (no video yet).
- The native hub.
- The **Plain** touchpad with the research's feel (A7: per-batch frames, low-latency Wi-Fi, palm marks, threshold haptics, tap mirror, truthful gesture labels, edge exclusion rects).
- The Keyboard: latching modifiers and the key row.
- Holds for the touchpad and keyboard: "In use by X — Take over?", a toast, Take back.

**P3b ships:**
- The shared layout model.
- Touchpad variants: Plain, Laptop, Edge scroll, Split, Precision, Media.
- The gamepad, with its variants and one pad per device.
- The shared editor, picker and comfort check.
- The surreal motion layer.
- Settings pages with live previews.
- On this phone → Gamepad.

**Tasks (one module each):**

P3a:
1. **daemon:** the pad link on control rooms; `holds/` for the touchpad and the keyboard, each with one holder, announced to all devices, with take over and take back (HANDOVER 0b).
2. Gradle: `stream-webrtc-android` at an exact pin, `abiFilters`, R8 keep rules; record the libwebrtc milestone in `docs/android.md`.
3. `rtc/RtcFactory.kt`: one `PeerConnectionFactory`, hardware decoder factory, `networkIgnoreMask = 0`.
4. `rtc/IceConfig.kt`: gathering policy, STUN from the daemon's list.
5. core `media/signal.rs`: offer, answer and trickle timing per F4.
6. core `pad/link.rs`: `PadOffer`/`PadAnswer`, channel roles.
7. `pad/PadPeer.kt`: the PC with `pad-touch`/`pad-keys`. It sends on a dedicated `HandlerThread` and is ViewModel-scoped, so it survives rotation (live issue L6).
8. `pad/InputBatcher.kt`: one frame per batch, stamped with `eventTime`, historical samples, contact size → palm.
9. `device/WifiLowLatency.kt`.
10. `device/Haptics.kt`: threshold kinds, API-gated with a `CLOCK_TICK` fallback.
11. `ui/input/Hub.kt` and `ui/input/HoldBanner.kt`.
12. `ui/input/touchpad/PlainSurface.kt`: rings, badge, gesture track, coach in its own space.
13. `ui/input/KeyboardScreen.kt`.

P3b:
14. `wado-protocol` `layout.rs`: the generalised `Layout`, with `kind` defaulting to `gamepad`. Additive pad messages: `Button{which, down}`, `SpeedHold{factor}`, and, only if Task 22's measurement says so, `Wheel{dx,dy}`. Gated by a `pad-variants` cap.
15. **daemon:** `Button` → `BTN_LEFT`/`BTN_RIGHT` on the touchpad device; `SpeedHold` → Hyprland sensitivity on `device[wado-trackpad]`, restored on release, on 2 s of silence and on link loss; config `trackpad { layout; mine }` with the self-device write rule and tests mirroring `device_gamepad.rs`.
16. core `layout/{model,place}.rs`: a port of `gpad/model.js` and `place.js` (`placePx`, §9.3).
17. core `layout/{gamepad_variants,touchpad_variants}.rs`: the built-ins; names carry no brand words.
18. core `pad/compose.rs`: control → pad messages (surface contacts; strip and wheel two-finger synthesis; clickbar; boost; ring → `HostDo`).
19. core `pad/game.rs`: GameOn, GameOff, Btn, Axes (resent every 100 ms while off-centre).
20. `ui/input/layout/Renderer.kt`: one renderer for both kinds, placing controls with the core's `placePx`.
21. `ui/input/layout/{Editor,Picker,Comfort,LookSheet}.kt`: one editor; the variant morph through shared elements per control id.
22. `ui/input/touchpad/{Trails,Afterglow,GestureTrack}.kt`: the surreal layer (A7 table), ring-buffer trails, a pre-rendered glow brush. Measure edge-scroll recognition with `libinput debug-events` before choosing synthesis or `Wheel`.
23. `ui/input/gamepad/GamepadScreen.kt`.
24. Settings pages: Touchpad (variant, look, speed, scroll, tap, drag lock), Keyboard, Gamepad, each led by a live preview; On this phone → Gamepad.

**Tests:**
- Rust:
  - `layout_without_kind_reads_as_gamepad`
  - `place_px_matches_js_fixtures` (fixtures exported from `scripts/padlayout-check.mjs`)
  - `touchpad_variants_are_valid_layouts`
  - `strip_synthesises_two_contacts_moving_together`
  - `clickbar_touch_never_becomes_a_contact`
  - `boost_released_on_silence` (daemon)
  - `hold_take_over_and_back`
  - `trickle_waits_for_srflx_or_700ms`
  - `mine_limits_8_and_4kb`
- Kotlin:
  - `one_frame_per_batch`
  - `eventtime_not_send_time`
  - `trail_buffer_never_allocates` (allocation count over 1,000 frames)
  - `tap_mirror_uses_mm`
- Emulator:
  - `HintOverlapTest`
  - `TouchpadTakeOverTest`
  - `VariantMorphInterruptTest`
  - `EditorDragResizeTest`
  - Macrobenchmark `TouchpadFrameTiming` with 5 fingers and trails: `frameDurationCpuMs` p95 ≤ 4 ms
- e2e: the pad PC against the release daemon over the local relay. `pad: arrivals` is read from the daemon log before and after the batching change (research M1/M2). Laptop's `Button` shows in `evtest`.
- Hardware:
  - the hotspot ICE check (A3)
  - research M4 (`libinput debug-events` for edge scroll and palm)
  - research M7 (240 fps camera)

**WebView still covers:** files, shells, the session, clipboard, push, Claude and widgets.

**Risks:**
- The libwebrtc size: measure it.
- The Android network monitor and the tether interface.
- Two-finger synthesis misread by libinput. The fallback is the daemon `Wheel` (Task 14).
- Hyprland-only speed boost: on other compositors, Precision hides Boost and says why.
- Trail cost on low-end GPUs: the look can be set to `trail off`, which is the default on devices whose measured `frameDurationCpuMs` p95 exceeds 6 ms.

## P4 — Files and the viewer (outline)

**Ships:**
- The native file manager: list, sort, search (`Find`), pins and recents, rename, move, copy, trash and restore, mkdir, uploads and downloads with progress and resume.
- The share sheet's file upload.
- The viewer for every kind in A5, with Details (a sheet on phones, a panel from 900 dp).

**Tasks:**
1. **daemon:** files on control rooms (the files PC signalled there)
2. core `files/kinds.rs`, a port of `viewer/kinds.js`, with a fixture test against its tables
3. core `files/ops.rs`
4. core `files/xfer.rs`: the binary frame `4-byte id ‖ ≤60 KiB`, resume by offset, writing to a platform fd
5. core `files/stream.rs` (`Stream` + `Credit`)
6. core `viewer/{md,json,csv}.rs`
7. `rtc/FilesPeer.kt`
8. `ui/files/{FilesScreen,Row,Toolbar,Transfers}.kt`
9. `ui/viewer/{ViewerScreen,Details}.kt` plus one file per kind: `ImageKind`, `MediaKind` (Media3 + `WadoStreamDataSource`), `PdfKind`, `TextKind`, `MarkdownKind`, `JsonKind`, `CsvKind`, `ArchiveKind`, `FontKind`, `InfoKind`
10. `share/` upload through the core

**Tests:**
- Rust: `kinds_match_js_tables`, `xfer_resume_from_offset`, `credit_never_exceeded`, `md_ast_*`.
- Emulator: `ViewerKindsTest` with `scripts/fixtures` files, `LaneMediaWaitsTest`.
- e2e: `files-e2e`-equivalent cases over the core (list, get, put, resume).

**WebView still covers:** shells, the session, clipboard, push, Claude and widgets.

**Risks:**
- ExoPlayer over a credit-paced, unseekable stream. Seek reopens; the scrubber offsets by `start`.
- HEIC decode on old devices. Fall back to the honest card.

## P5 — Shell (outline)

**Ships:** native shells. List them, open a new one or attach, one per device (parallel use), with the extra-keys row, Termux view, base16 colours and JetBrains Mono.

**Tasks:**
1. **daemon:** shells on control rooms
2. Vendor `terminal-emulator` and `terminal-view` (Apache-2.0, LICENSE and NOTICE kept) as Gradle modules
3. `RemoteTerminalSession` patch (one commit)
4. core `shell/{pty,list}.rs`
5. `ui/shell/{ShellScreen,ExtraKeys,ShellList}.kt`
6. About: licences

**Tests:**
- Rust: `pty_output_order`, `resize_sends_pty_resize`.
- Kotlin: `RemoteTerminalSessionTest` (bytes in → emulator screen text).
- Emulator: `ShellImeTest` (Ctrl+C via extra keys).
- e2e: `shells-e2e`-equivalent (echo roundtrip) over the core.

**WebView still covers:** the session, clipboard, push, Claude and widgets.

**Risks:**
- IME quirks per keyboard app (Gboard, SwiftKey). Test both.

## P6 — The session: video, audio, input, console and cockpit (outline)

**Ships:**
- **Desktop joins by default** (a view room plus `SessionJoin`); Start my own; Take over for size.
- `SurfaceViewRenderer` on Stage Black, with H.264 preferred and H.265/AV1 only with hardware decode.
- Opus audio.
- Input on `wado-input`, `wado-motion` and `wado-fast` from a dedicated thread.
- The Pocket Console: top band, rails, window strip, deck faces reusing P3's pieces, dock row.
- The cockpit morph S1–S13.
- The control centre.
- Apps and the menu sheet.
- The software-encode notice (invariant 5).
- Autorate and stats.
- Input feel and Quick tiles pages.

**Tasks:**
1. **daemon:** "start my own" from a view room; `playout-delay` extension behind a Display & stream switch
2. core `session/intent.rs` ("left means left", `session_detach` on a late start)
3. core `session/input.rs` (InputEvent codecs, compact frames, `fast` redundancy with `q` sequence numbers)
4. core `session/stats.rs`
5. `rtc/SessionPeer.kt` (transceivers, codec preferences, channel set, jitter floor)
6. `ui/session/Stage.kt` (SurfaceView, in-stage states, first-frame reveal)
7. `ui/session/input/{Pointer,Touch,Keys,Ime}.kt` (mapping against the picture's transformed rect at event time)
8. `ui/session/console/{TopBand,Rails,WindowStrip,Deck,DockRow}.kt`
9. `ui/session/Cockpit.kt` + `Morph.kt` (`LookaheadScope`)
10. `ui/session/ControlCentre.kt` + `Tiles.kt`
11. `ui/session/Apps.kt`
12. `ui/phone/{InputFeel,QuickTiles}Screen.kt`
13. Immersive mode, orientation, screen-on (`device/ScreenModes.kt` reused)

**Tests:**
- Rust: `late_start_is_detached`, `input_codec_matches_protocol_json`, `fast_dedup_seq`.
- Emulator: `ConsoleImeTest`, `StageFirstFrameTest`, `StageStatesTest`, `SessionBackTest`, `MorphInterruptTest`, `TapMidMorphLandsUnderFinger`.
- e2e: a session against a release daemon over the local relay with `video-attach`/`session-e2e` cases ported; `getStats` decode and jitter numbers logged.
- Hardware:
  - glass-to-glass vs the WebView on the same phone, 240 fps camera, LAN and relayed
  - `decode_drop_pct` in console vs cockpit (the pocket-console spec's open measurement)

**WebView still covers:** clipboard history, push, Claude, widgets and the link test.

**Risks:**
- Glass-to-glass regressions hidden by a different render path. Measure first, then ship.
- SurfaceView inside a transformed parent: a SurfaceView does not take `graphicsLayer` scale.
  - The morph uses `SurfaceView.setScaleX/Y` (API 29+ composes it in SurfaceFlinger) or `TextureView` for the inset picture.
  - Decide by measurement in the first P6 task. Keep SurfaceView in the cockpit either way.

## P7 — Clipboard, push, Claude, multi-client, the lane, widgets (outline)

**Ships:**
- The clipboard surface and history, the chip and voice. `ClipSync.kt` stays as the phone side, fed by the core instead of the page.
- UnifiedPush registration from the core (`PushSub` kind `unifiedpush`).
- The Claude card and picker; notification Allow/Deny answered by the core with the passkey.
- The people chip, ghost pointers on `wado-presence`, join and leave toasts.
- Lane hints with Retry direct.
- Link and speed test.
- Home widgets with edit mode (M13–M15).
- Share sheet links and text.

**Tasks:**
1. **daemon:** Claude and push registration on control rooms
2. core `clip/{share,history}.rs`
3. core `push.rs`
4. core `claude.rs`
5. core `session/presence.rs`
6. core `linktest.rs`
7. `ui/clipboard/*`
8. `push/Push.kt` rewired
9. `ui/claude/*`
10. `ui/session/People.kt` + `Ghosts.kt`
11. `ui/common/LaneHint.kt`
12. `ui/home/widgets/*` + `Edit.kt`
13. `notify/Notifier.kt` actions → core

**Tests:**
- Rust: `secret_clip_never_sent`, `echo_hash_suppresses`, `push_sub_shape`, `claude_allow_needs_passkey`.
- Emulator: `WidgetEditTest`, `ClipChipTest`.
- e2e: `clip-e2e` and `f3-e2e`-equivalent over the core.

**WebView still covers:** nothing a user needs. "More (web)" remains for one release as a safety hatch.

**Risks:**
- Push decrypt stays in the existing connector (RFC 8291); verify it on a locked phone.

## P8 — Delete the WebView (outline)

**Tasks:**
1. Delete `web/`, `bridge/`, `ClientServer.kt` and `AssetPaths.kt`, and the `crates/client/src/native*.rs`/`js/native*.js` app paths that are now dead. The web client keeps working in browsers; `native.js` feature-detects.
2. Delete the `dx build` step and `assets/client/` from `scripts/android-build.sh`, and `androidx.webkit` from Gradle.
3. Delete "More (web)", and the legacy-card merge in `CoreState`.
4. R8 on for release (keep rules for UniFFI/JNA and `org.webrtc`), per-ABI splits, APK size measured against A8.
5. Update `docs/android.md` (the bridge table removed), `PRODUCT.md` Operating Context (the app is native), DESIGN.md (the Compose implementation notes), and the Decision Log.
6. A one-time screen on each computer's page: "This phone also has an older web-part pairing on <computer>. Remove it there with `wado tui` → Devices." This is honest and needs no remote approval.

**Tests:**
- `rg -n "WebView|wadoNative" android/` is empty.
- All emulator suites pass.
- APK size and startup re-measured.

**Risks:** none new. The rollback is the previous release's APK.

---

## Rough sizes

| Phase | Size | Notes |
|---|---|---|
| P1 | L: 18 tasks, one agent run | mostly Rust plus the home; two tiny daemon edits |
| P2 | L | config views are the bulk |
| P3 | L+, as P3a (transport, Plain touchpad, keyboard, holds) and P3b (layout model, variants, editor, gamepad, surreal layer) | libwebrtc lands |
| P4 | M–L | eleven viewer kinds |
| P5 | M | vendoring plus a patch |
| P6 | XL | split into P6a (stage, video, input, cockpit) and P6b (console, control centre, apps) if one run cannot hold it |
| P7 | M | |
| P8 | S | |

## Open questions — resolved here

| Question | Resolution | Why |
|---|---|---|
| Which libwebrtc build? | `io.getstream:stream-webrtc-android`, exact pin; Apache-2.0 + BSD-3 | Maintained on Maven Central and AGPL-compatible. Google's own `org.webrtc:google-webrtc` has been unpublished since 2020. |
| Share the core with desktop? | Yes: `client-core` is pure Rust; `mobile-core` is the UniFFI façade | Desktop needs no FFI; one protocol implementation. |
| Can we migrate the WebView's pairings? | No. Re-pair once per computer | The key is a non-extractable WebCrypto key. A trust-model change for a transitional problem was rejected. |
| Pair on a control room? | No. Seat join for the ceremony only, then `4001` | The daemon refuses pairing on control rooms by design. |
| Passkey origin in the native app | Keep `https://appassets.androidplatform.net` | The daemon already accepts it. The Keystore authenticator is unchanged. |
| Terminal | Termux modules, vendored | Mature IME handling; Apache-2.0. |
| Logos and trademarks | Simple Icons, pinned, licence-checked per icon, monochrome base16 tint, nominative use only | CC0 project; the brand guidelines link is listed on About. |
| Size: phone or computer? | Phone-local, mirrored best-effort | It is this phone's chrome; the computer copy existed because browsers lose storage. |
| How events cross UniFFI | Pull (`suspend nextEvent`) with per-key coalescing | No JVM callbacks from Rust threads; natural backpressure. |
| Touchpad variants: how strips scroll and how Precision is slower | Two-finger synthesis on the touchpad device, measured with `libinput debug-events`, with the daemon `Wheel` as fallback. Speed through Hyprland only. | It keeps libinput's thresholds and the computer's scroll settings honest (research rec. 3); no new uinput device unless measurement demands one. |
| The SurfaceView in the console's scaled picture | Measure in P6's first task: `setScaleX/Y` vs a TextureView for the inset | SurfaceView keeps the overlay path in the cockpit, where latency matters most. |

## Open questions — left for the user (none block P1)

1. **TURN on the relay** as the honest fallback when no direct path exists (HANDOVER item 1). It costs relay bandwidth beyond the 64 KB/s lane's intent. This is needed only if P3's hotspot check fails.
2. **Remove "More (web)" in P8, or keep it one more release?** The plan removes it in P8, as decided ("the WebView is deleted last").

---

## Addendum (`2026-10-31`): live events over wado's own connection (user request)

The user, verbatim: "I want to send notifications directly over wado connection or some other way
that can use push events style structure to deliver those and sync playback properly over it too
live, if multiple hosts are connected then multiple playbacks and notifications labelled ones
should appear accordingly."

**One event stream per computer, delivered over wado's own link. No FCM, no third party.**

- **Shape:** typed, sequenced events from each daemon, sealed end to end like everything else:
  - `notify {id, kind, title, body, actions[], urgency}`: Claude needs permission or finished,
    transfer done, computer online.
  - `playback {player, title, artist, art_hash, state, position_ms, rate, at_ms, can{…}}`. The
    phone extrapolates the position from `at_ms` and `rate`, so the scrubber is live without a
    stream of updates. A new event comes only when something changes (track, play/pause, seek,
    rate) and on a resync.
  - `volume {level, muted}`, `presence {online}`, `clip_ready {…}`, …
  - Each event carries `seq`. On reconnect the phone asks "since seq N", and the daemon replays
    from a small ring (last ~200), so nothing is missed across a blip.
- **Delivery while the app is in the background:** the app keeps a long-lived link (a
  `remoteMessaging` foreground service with a silent ongoing notification, keepalives every few
  minutes). With several computers the efficient structure is **one socket for all of them**: a
  relay *device inbox* (new additive cap `inbox`). Each daemon posts sealed events to the device's
  inbox, and the relay forwards them unread to that device's single background socket, or holds a
  small backlog for it. The fallback for an older relay is one control link per computer. The
  relay also posts an unsealed `host_gone {remote_id, instance}` when a daemon drops: the only
  part that can say "bushido stopped answering". UnifiedPush stays as an optional wake-up.
- **On the phone:**
  - One **Android notification channel group per computer**, labelled with its name and OS
    logo.
  - Notifications carry the computer's name, and their actions (Allow/Deny, Open) go back over
    the link.
  - **One MediaSession per computer that is playing**, so the lock screen and quick settings show
    one player card per computer ("Spotify · bushido", "mpv · laptop2"), each with its own
    controls, position and art.
  - Volume keys drive the computer whose card was used last; this is settable.
- **Where it lands:**
  - P1: the core's link supports a long-lived background mode, with configurable keepalive and
    multiplexed hosts.
  - P7: the events protocol (daemon `events/` with ring and seq), the relay `inbox` cap, the
    foreground service, channels per computer, MediaSessions per computer, and resync.
  - The web client can use the same stream while it is open.
- **Tests:**
  - an e2e with two daemons and one phone core: interleaved notifications, both labelled;
  - two playback events → two sessions;
  - a dropped link → resync from seq with nothing lost or duplicated;
  - position drift under 250 ms over 5 minutes without updates.
