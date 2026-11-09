# wado App (Flutter) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** One native app for Android, iOS, iPadOS, macOS and Linux, written in Flutter over the Rust client core. It replaces the Android app's WebView and the Compose P1 screens, and it never hosts the web client.

**Architecture:**
- **Rust:** `crates/client-core` holds the whole client protocol. It already exists (P1 of the Compose plan): relay link, E2E, pairing, control rooms, feeds and request deadlines. A new `crates/app-bridge` exposes it to Dart through `flutter_rust_bridge`.
- **Flutter:** draws every screen.
- **Video and data channels:** `flutter_webrtc` on all five platforms. Signalling goes through the core.
- **Platform features:** Android libraries are called straight from Dart through `jni`/jnigen. Apple frameworks use ffigen/swiftgen.
- **Ship order:** Android first. Linux is the daily dev loop from F0. Apple platforms come in F8.

**Tech Stack:**
- **Flutter and Dart:**
  - Flutter 3.47.0 (the flake's pinned nixpkgs).
  - `flutter_rust_bridge` 2.13.0: the crate, the Dart package and the codegen, all exactly equal.
  - `flutter_webrtc`.
  - `go_router`.
  - `shared_preferences`.
  - `flutter_secure_storage`.
  - `flutter_svg`.
  - `flutter_zxing`.
  - `app_links`.
  - `jni` + `jnigen`.
  - Later phases: `xterm` (F5), `pdfrx` and `media_kit` (F4).
- **Rust:** `wado-client-core` (exists), `wado-protocol`, `ring` 0.17.
- **State:** the SDK only (`ChangeNotifier`, `ValueListenableBuilder`). No state-management package.

**Spec:**
- Product design: `docs/superpowers/plans/2026-10-30-native-android.md` **Part A**. It still holds wherever it is not framework-specific:
  - A1 holds/changes
  - A2 the core and link model
  - A5's kinds table
  - A6 settings pages
  - A7 UX: home, cards, quick lip, On this phone, per-computer pages, virtual input, accessibility, action feedback, WebView defects
  - A8 budgets
  - A9 decisions, except 1, 7, 8, 9 and 12, which this plan replaces
  - A10 parity
- Decision Log `2026-10-31`: "One UI framework for every platform: Flutter".
- Also: `PRODUCT.md`, `DESIGN.md`, and the specs and research listed in that plan's header.

Executors read Part A of that plan, then Part A of this one, before any task.

---

# Part A — What changes for Flutter

## A1. Repo layout

```
app/                               the Flutter project (name: wado, org: io.github.sandptel)
  pubspec.yaml, pubspec.lock       every package pinned exactly; lockfile committed
  lib/main.dart                    runApp only
  lib/bridge/                      flutter_rust_bridge output — generated, never edited
  lib/core/                        CoreHost and the Dart-side models over core events
  lib/nav/                         routes and restoration
  lib/ui/theme/                    base16 roles, schemes, type, motion tokens, SizeScope
  lib/ui/common/                   Pending, Tap, LinkWords, OsDisc, ThickSlider, ToggleTile …
  lib/ui/<area>/                   home, pair, phone, computer, input, files, shell, session …
  lib/platform/<feature>.dart      one Dart interface per platform feature; per-OS impls beside it
  lib/platform/android/gen/        jnigen output — generated, never edited
  assets/{os,fonts,schemes}/       OS logos (SVG), Bricolage Grotesque + JetBrains Mono, schemes.json
  android/ ios/ macos/ linux/      Flutter host projects
  android/app/src/main/kotlin/…    the Kotlin kept from android/ (A6), unchanged in behaviour
  test/ integration_test/
crates/app-bridge/                 cdylib + staticlib; the only crate Dart sees
  src/api/{mod,core,event,secret,hot}.rs
scripts/app-build.sh               builds the Rust library for a target, then `flutter build`
scripts/app-e2e.sh                 relay + release daemon + integration_test on linux or the emulator
```

**Removed at F9:**
- `android/`, the old Kotlin/Compose shell with its WebView and web bundle.
- `crates/mobile-core`, the UniFFI façade.

Until then:
- The old app keeps building, so the user always has a working app.
- The new app installs beside it as **`io.github.sandptel.wado.next`** (label "wado next").

## A2. The bridge (`crates/app-bridge`)

- **The pin is exact on both sides,** like `wasm-bindgen`. `flutter_rust_bridge = "=2.13.0"` in the crate must equal the `flutter_rust_bridge: 2.13.0` package and the codegen (nixpkgs `flutter_rust_bridge_codegen` 2.13.0). They move together or the build dies.
- **Commands** are plain sync functions returning at once, as in the core: `watch(keys)`, `pair(link) -> u64`, `forget(key) -> u64`.
- **Events go push, not pull.**
  - `fn events(sink: StreamSink<CoreEvent>)` spawns one task that loops `core.next_event().await` into the sink.
  - frb posts to the Dart isolate's port, so the core still never calls into Dart from its threads (A9.12's rule, kept).
  - Coalescing stays in the core's bounded queue.
- **Hot paths** are `#[frb(sync)]` pure functions in `api/hot.rs`: pad and input encoding, file-frame parsing. Nothing on the input path awaits the runtime (invariant 1).
- **Mirror types:** `api/event.rs` holds flat mirror structs. This is the same job `mobile-core/src/ffi/event.rs` does today; port it, don't import it.

## A3. Secrets: one key in the platform keystore

The core's `SecretBox` trait stays. The bridge implements it as `AesBox`: AES-256-GCM (`ring`) with a random 32-byte **box key**.
- **Where the box key lives:** `flutter_secure_storage`, under `wado.box.v1`.
  - Android: Keystore-backed.
  - iOS and macOS: Keychain.
  - Linux: libsecret.
- **Startup:** Dart reads the key, or creates and stores one, and passes it to `Core::new` once.
- **What reaches disk:** only wrapped bytes, and the key itself is only ever in the keystore. This matches today's property under a different holder.
- **Linux with no Secret Service:**
  - The app refuses to start the core and says why.
  - Debug builds only may use `--dart-define=WADO_DEV_BOX=<path>`, a 0600 file, so `flutter run -d linux` works on a bare compositor.
  - Release builds ignore the define (a test pins this).
- **A lost box key** (keystore wiped): the wrapped identity can't be opened. The core starts as a **new device**, keeps the old file aside as `identity.lost`, and home says "This phone's key was reset — pair your computers again". It never crashes or loops.

The Flutter app is a new device. Each computer is paired again, once, exactly as the Compose plan's A9.5.

## A4. Media: `flutter_webrtc` on every platform

The signalling, channels, ICE policy and latency knobs of Compose plan A3 all hold. Its "Pick A" table becomes this plan's choice of `flutter_webrtc`, which wraps the same Google libwebrtc on each platform.
- **Signalling:** Dart creates the PC, transceivers and channels (`wado-input`, `wado-motion`, `wado-fast`, `wado-presence`), then calls `core.offer(key, sdp)`. The answer and ICE come back as core events. Local candidates go out through `core.ice(key, cand)`, using F4's hold rule, implemented in the core.
- **ICE on every interface:** `{'candidateNetworkPolicy': 'all', 'continualGatheringPolicy': 'gather_continually'}`, with STUN from the daemon's list.
- **Decode:**
  - Android: MediaCodec through libwebrtc's hardware factory.
  - Apple: VideoToolbox.
  - **Linux:** libwebrtc's software decode. This is a known ceiling; measure in F8, and note it in `docs/app.md`.

**Risk R1, the rendering path.** `RTCVideoView` draws through a Flutter `Texture`, not a `SurfaceView` overlay, which may cost a compositor frame.
- **Gate (F6):** glass-to-glass p50 must be at most the WebView's p50 (Compose plan A8).
- **If it fails:** an Android `PlatformView` (Hybrid Composition) holding a `SurfaceViewRenderer` bound to the same libwebrtc track, which `flutter_webrtc` exposes by id, reached through jnigen. It ships as a Display & stream switch ("optimisations as options").

**Risk R2, the input path.**
- **The path:** `DataChannel.send` in `flutter_webrtc` crosses a platform channel.
- **Gate (F3):** event → send p90 ≤ 2 ms, the same budget as A8.
- **If it fails:** the **pad link** is its own PC, so it can move into the core: webrtc-rs 0.17 data channels only, with no media. Dart then calls a sync `pad_send(bytes)`.
- **Session input** shares a PC with video and stays on `flutter_webrtc`. Its own gate is F6.

**Jitter-buffer floor:** `setJitterBufferMinimumDelay` is not part of `flutter_webrtc`'s API. Reach the receiver through jnigen on Android. Only if F6's numbers show the default floor costs latency, carry a ≤ 30-line patch on a fork pinned by git rev.

## A5. The terminal and the viewer

| Need | Pick (licence) | Note |
|---|---|---|
| Terminal (F5) | `xterm` (MIT) | Feeds `terminal.write` from the core's `PtyOutput`; `onOutput` → `core.pty_input`. An extra-keys row of our own. base16 → `TerminalTheme`, JetBrains Mono. This replaces the vendored Termux modules (Compose A4). |
| Images | SDK `Image` + `InteractiveViewer` | No package. SVG via `flutter_svg`. A kind a platform can't decode falls to the honest info card. |
| PDF | `pdfrx` (MIT, PDFium) | All five platforms. |
| Video and audio | `media_kit` (MIT, libmpv) | Plays `http://127.0.0.1:<port>/s/<id>` from a **loopback range server in the core** (`files/serve.rs`, F4). The server is fed by the files PC's bytes via `#[frb(sync)] files_feed(id, bytes)` and honours `Credit`. A seek opens a new `Stream{start}`. |
| Markdown, JSON, CSV | Parsed in the core (Compose A5) | Dart only renders the block or row trees. |
| Fonts | SDK `FontLoader` | |

## A6. Platform features (`lib/platform/`)

- **One interface per feature,** with an implementation per OS. `lib/platform/haptics.dart` holds `Haptics` and its `android.dart` / `apple.dart` / `none.dart`.
- **A missing implementation is honest:** the feature is greyed out with a reason, per DESIGN, never a crash.
- **Kept Kotlin.** These are called through jnigen, with their behaviour unchanged. They move from `android/app/src/main/java/io/github/sandptel/wado/{passkey,stay,clip,notify,push,share,shortcuts}` to `app/android/…`:
  - `Authenticator.kt` and `AuthErrors.kt` (passkeys, origin per Compose A9.6)
  - `StayConnectedService.kt`
  - `ClipSync.kt`
  - `Notifier.kt`
  - `Push*.kt`
  - share target and shortcuts
- **The two-way control the user plans** (the computer asking the phone to do things) arrives later as a core event `PhoneDo{action}`, dispatched by `lib/platform/actions.dart` to these interfaces. No task builds it now. The layout above is what keeps it a small addition.

## A7. UX in Flutter

Compose plan A7 holds. The Flutter forms:
- **Navigation:** `go_router` with these routes:
  - `/`
  - `/pair`
  - `/pair/typed`
  - `/phone/:page`
  - `/c/:key`
  - `/c/:key/:page`
  - `/c/:key/pad`
  - `/c/:key/session`

  Each route is restorable (`restorationScopeId`). Android predictive back comes from `PredictiveBackPageTransitionsBuilder`. **`/c/:key/session` is pushed only from a tap handler**, never from an event listener. A test pins this (WebView defect 5).
- **Pages are opaque,** so the previous page never shows through (the user's layering report, as WebView defect 8): `MaterialPage`/`CustomTransitionPage` with `opaque: true`, and sheets with a scrim. Test: `PagesAreOpaqueTest`.
- **SizeScope:**
  1. `MediaQuery(size: size / ui, padding: padding / ui, textScaler: TextScaler.linear(text))`, then `Transform.scale(scale: ui, alignment: Alignment.topLeft)`.
  2. Hit testing follows the transform.
  3. It wraps pages and sheets only, never the stage, console, pad or gamepad.
  4. `Tap` makes every target `44 / ui` logical px, so it is at least 44 on the glass.
  5. Defaults: Cozy (1.0), text Follow phone.
- **Motion:** Compose's terms map to Flutter as follows; the durations and the two curves come from DESIGN.md.
  - `lib/ui/theme/motion.dart` holds `Motion.spring` (a `SpringDescription`), `Motion.pop` and the duration tokens.
  - M2/M4 is `Hero` on the disc and name, plus a custom route for the card → hero surface.
  - M17 is a custom `PageTransitionsBuilder` (push 100 % → 0; behind drifts −24 % and fades).
  - Animate only transform and opacity.
  - The motion level (full / reduced / off) is AND-ed with `MediaQuery.disableAnimations`.
- **Action feedback:** `lib/ui/common/pending.dart`.
  - pressed < 100 ms
  - working by 300 ms
  - taking long past the action's threshold, with Cancel and Try again
  - the outcome

  It is driven by the core's `Answer`/`NoAnswer` events.

## A8. Budgets

Compose plan A8 holds, with these tools:
- `flutter run --profile --trace-startup` for `timeToFirstFrameMicros`.
- `integration_test` with `traceAction` for frame build and raster p95.
- Android Macrobenchmark still runs against the APK for cold start.

**APK:** ≤ 32 MB arm64 after F6, and ≤ 30 MB after F9. The Flutter engine is about 5 MB and libwebrtc about 10 MB. Measured by `scripts/android-budget.mjs`, retargeted.

## A9. Decisions resolved here (each becomes a Decision Log line when its phase executes)

1. **Bridge:** `flutter_rust_bridge` 2.13.0, exact pin on crate, package and codegen. `crates/app-bridge` replaces `crates/mobile-core` (deleted in F9).
2. **Events** are pushed through a `StreamSink` fed by one task looping `next_event()`.
3. **Secrets:** `AesBox` with a box key in `flutter_secure_storage`. On Linux, no Secret Service means a refusal, except for a debug-only key file.
4. **Media:** `flutter_webrtc` everywhere. The PlatformView SurfaceView path is the measured fallback (R1). Moving the pad link into the core is the measured fallback for input (R2).
5. **Terminal:** `xterm`. **Viewer:** SDK image and `InteractiveViewer`, `pdfrx`, `media_kit` over a loopback range server in the core.
6. **QR:** `flutter_zxing` (zxing-cpp). Not ML Kit-based scanners: no closed binaries in an AGPL app.
7. **Navigation:** `go_router` with restoration and predictive back.
8. **State:** SDK notifiers only.
9. **Side-by-side install:** `io.github.sandptel.wado.next` until F9, when it takes `io.github.sandptel.wado`.
10. **Build:** the existing cargo-ndk route (`scripts/android-core.sh`) writes `jniLibs/`. Linux bundles the `.so` from CMake. Apple uses an xcframework (F8). No cargokit.

---

# Part B — Plan

## Global Constraints

- AGPL-3.0-only on `crates/app-bridge` and the Flutter project (`LICENSE` in `app/`). Every package's licence is listed on About (`LicenseRegistry` plus `assets/licenses/`).
- **Invariants 1, 2, 4 and 5** (`CLAUDE.md`):
  - Input encodes on a sync bridge call and sends from the input handler, never behind the event stream or video.
  - Video only over WebRTC/UDP.
  - H.264 first in codec preferences.
  - The software-encode notice is shown when the daemon reports it.
- **The relay handshake is frozen.** Daemon additions are additive `#[serde(default)]` fields. **The compositor/server boundary holds.**
- **No seat-gated feature.** The seat is used only for the pairing ceremony (Compose A9.4).
- One module, one job. Dart files ≤ about 300 lines. Split on a second job. Platform checks (`Platform.isX`) only inside `lib/platform/`.
- Every colour from a base16 role. Stage Black only behind video. Two curves plus the accelerate fade ≤ 180 ms. Duration tokens from DESIGN.md. Targets ≥ 44 on the glass. Text inputs ≥ 16 on the glass.
- Plain words first; technical names one tap away (Diagnostics).
- **Clean before every commit:**
  - `cargo fmt --all --check`
  - `dart format --set-exit-if-changed app/lib app/test app/integration_test`
  - `flutter analyze` with zero issues
  - Generated folders (`lib/bridge/`, `lib/platform/android/gen/`) are excluded from format and analyze.
- No AI attribution in commits. Short commit messages.
- **Builds:**
  - Rust tests: `cargo test -p wado-client-core -p wado-app-bridge`.
  - Dart tests: `nix develop .#app -c bash -c 'cd app && flutter test'`.
  - Linux run: `nix develop .#app -c scripts/app-build.sh --linux --debug`.
  - APK: `CARGO_TARGET_DIR=$PWD/target-android nix develop .#app -c scripts/app-build.sh --android --debug`.
  - GPU, emulator and Linux-window work needs the Bash sandbox off.
- The daemon in any e2e is a **release** build.

## Review Focus

1. **The app goes to the background and comes back.** The Dart isolate pauses and the event stream backs up. Expect: on resume, cards show the newest state for each computer, no event older than one already shown is applied, and nothing throws. Test: `CoreHostTest.resume_applies_only_newest_per_slot` (Task F1.4).
2. **The keystore was wiped, or the box key is missing.** Expect: a new device identity, `identity.lost` kept aside, and home says "This phone's key was reset — pair your computers again". No crash, no loop. Test: `lost_box_key_starts_fresh_and_says_so` (Task F0.4).
3. **Two cores on one data dir** (a debug hot restart, or a second window on desktop). Expect: the second `Core::new` fails with "wado is already running", and the first keeps working. Test: `second_core_on_same_dir_refuses` (Task F0.2).
4. **Large text and Large interface on a 360×640 phone.** Expect: no overflow on home, a card, Pair or Appearance. Text wraps or ellipsizes, and targets stay ≥ 44 on the glass. Test: `SizeMatrixTest` at 150 % × 1.2 (Task F1.2).
5. **A pairing link opened while the app is cold or on another page.** Expect: Pair opens with the link filled in and starts once, and a restore after process death does not start it again. Test: `PairDeepLinkTest.cold_and_warm_start_once` (Task F1.7).

---

## F0 — Foundation: the bridge, the dev shell, builds on Linux and Android

**Ships:** "wado next" opens on Linux and on the emulator, starts the Rust core and shows this device's client key. Nothing else. This is the base every later phase builds on.

### Task F0.1: The `.#app` dev shell

**Files:**
- Modify: `flake.nix` (add `appShell` next to `androidShell`; expose `devShells.app`)
- Create: `docs/app.md` (how to build and run, with pins and their reasons)

**Interfaces:**
- Produces: `nix develop .#app`, which provides everything in the android shell plus `flutter` 3.47.0, `flutter_rust_bridge_codegen` 2.13.0, `pkg-config`, `gtk3`, `libsecret`, `cmake` and `ninja` (Linux desktop).

- [ ] **Step 1:** Add the shell, reusing the android shell's SDK composition. Set `FLUTTER_ROOT`, `ANDROID_HOME` and `CHROME_EXECUTABLE=` (empty; there is no web target).
- [ ] **Step 2: Verify:** `nix develop .#app -c flutter doctor -v`. Expected: Android toolchain ✓ and Linux toolchain ✓. Web and iOS are not needed.
- [ ] **Step 3: Verify the jni pin.** Run `nix develop .#app -c dart pub add --dry-run jni jnigen` in a scratch project. If `jni` 1.x needs a newer Dart than 3.47 ships, overlay `flutter` from nixpkgs-unstable for this shell only, pinned by rev. Record the decision in `docs/app.md`.
- [ ] **Step 4: Commit:** `app: dev shell for Flutter`.

### Task F0.2: `crates/app-bridge` and the generated Dart

**Files:**
- Create: `crates/app-bridge/{Cargo.toml,src/lib.rs,src/api/{mod,core,event,secret}.rs}` and `flutter_rust_bridge.yaml`
- Create: `app/` via `flutter create --org io.github.sandptel --project-name wado --platforms android,linux app`, then trim the counter sample
- Create: `app/lib/main.dart`, `app/lib/core/core_host.dart`
- Modify: `Cargo.toml` (workspace member)
- Modify: `crates/client-core/src/core.rs` (an exclusive lock on the data dir)
- Test: `crates/client-core/src/core.rs` (`second_core_on_same_dir_refuses`)
- Test: `crates/app-bridge/src/api/event.rs` (mirror tests)

**Interfaces:**
- Consumes: `wado_client_core::core::Core::{new, client_key, computers, watch, set_visible, set_keepalive, pair, pair_typed, forget, next_event, shutdown}` and `os_logo::slugs()`. Read `crates/mobile-core/src/ffi/core.rs` for the façade shape.
- Produces (Rust, `api/core.rs`; Dart names are frb's camelCase):
  - `pub struct AppCore` (opaque)
  - `AppCore::start(dir: String, box_key: Vec<u8>, device_name: String) -> Result<AppCore, String>`
  - `fn client_key(&self) -> String`
  - `fn computers(&self) -> Vec<ComputerInfo>`
  - `fn watch(&self, keys: Vec<String>)`
  - `fn set_visible(&self, on: bool)`
  - `fn pair(&self, link: String) -> u64`
  - `fn pair_typed(&self, relay: String, remote_id: String, code: String) -> u64`
  - `fn forget(&self, key: String) -> u64`
  - `fn events(&self, sink: StreamSink<AppEvent>)`
  - `fn shutdown(&self)`
  - `#[frb(sync)] pub fn os_logo_slugs() -> Vec<String>`
- Produces (`api/event.rs`): `ComputerInfo`, `LinkInfo`, `Tone`, `AppEvent`. These are a field-for-field port of `mobile-core/src/ffi/event.rs`'s `ComputerInfo` and `CoreEvent`.
- Produces (Dart): `class CoreHost extends ChangeNotifier { static Future<CoreHost> start(); AppCore get core; }`

- [ ] **Step 1: Failing test** in client-core, `second_core_on_same_dir_refuses`. `Core::new(dir)` twice. The second returns `Err` containing `"already running"`, and the first still answers `client_key()`.
- [ ] **Step 2: Run** `cargo test -p wado-client-core second_core`. Expected: FAIL.
- [ ] **Step 3: Implement** the lock: `std::fs::File::try_lock` (stable since 1.89) on `<dir>/core.lock`, held by `Core`.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Mirror tests** in `api/event.rs`:
  - `computer_info_carries_every_view_field`: build a `ComputerView` with every field set; the mirror equals it field by field.
  - `every_core_event_maps`: one `CoreEvent` of each variant maps without a panic, and `slot()` survives.
- [ ] **Step 6: Implement** the crate.
  - `crate-type = ["cdylib", "staticlib", "rlib"]`.
  - Pin `flutter_rust_bridge = "=2.13.0"`.
  - Run `flutter_rust_bridge_codegen generate` (`rust_input: crate::api`, `dart_output: app/lib/bridge`).
  - `AppCore::start` takes a stub box for now; Task F0.4 replaces it.
- [ ] **Step 7: Run** `cargo test -p wado-app-bridge`. Expected: PASS.
- [ ] **Step 8: Commit:** `app: bridge crate and generated Dart`.

### Task F0.3: Builds and the first run on Linux and Android

**Files:**
- Create: `scripts/app-build.sh` (`--linux|--android`, `--debug|--release`)
- Modify: `scripts/android-core.sh` (a `--crate` and `--out` argument, so it builds `wado-app-bridge` into `app/android/app/src/main/jniLibs`)
- Modify: `app/linux/CMakeLists.txt` (install `libwado_app_bridge.so` into the bundle's `lib/`)
- Modify: `app/android/app/build.gradle.kts`:
  - `applicationId "io.github.sandptel.wado.next"`
  - label "wado next"
  - `abiFilters` arm64-v8a and x86_64
- Create: `app/integration_test/boot_test.dart`

**Interfaces:**
- Consumes: `CoreHost.start()` (F0.2).
- Produces: a home placeholder (`lib/ui/home/home_screen.dart`) that shows `core.clientKey()`, to be replaced in F1.6.

- [ ] **Step 1: Failing integration test** `boot_test.dart` → `starts_core_and_shows_client_key`. A widget showing text matching `^[A-Za-z0-9-]{8,}$` appears within 3 s. This is the relay's key alphabet; `_` is stripped, per `plan/memory/shared/environment.md`.
- [ ] **Step 2: Run** `nix develop .#app -c bash -c 'cd app && flutter test integration_test/boot_test.dart -d linux'`. Expected: FAIL (no build yet).
- [ ] **Step 3: Implement** `app-build.sh`:
  - Linux: `cargo build -p wado-app-bridge [--release]`, then `flutter build linux`.
  - Android: `android-core.sh --crate wado-app-bridge --out app/android/app/src/main/jniLibs`, then `flutter build apk --split-per-abi`.
  - Copy the arm64 APK to `~/Downloads/wado-next-<version>-<debug|release>.apk`.
- [ ] **Step 4: Run** the Linux test. Expected: PASS. Then on the emulator: `nix develop .#android-emulator -c scripts/android-emulator.sh` and `flutter test integration_test/boot_test.dart -d emulator-5554`. Expected: PASS.
- [ ] **Step 5: Commit:** `app: Linux and Android builds; boot test`.

### Task F0.4: The box key

**Files:**
- Create: `crates/app-bridge/src/api/secret.rs` (`AesBox`)
- Create: `app/lib/core/box_key.dart`
- Modify: `crates/client-core/src/identity.rs` (handling for an unopenable identity)
- Modify: `crates/client-core/src/event.rs` (`CoreEvent::KeyReset`)
- Test: `secret.rs`, `identity.rs`, `app/integration_test/boot_test.dart`

**Interfaces:**
- Produces:
  - `pub struct AesBox { key: [u8; 32] }` implementing `wado_client_core::secret::SecretBox`.
  - The sealed format: a 12-byte random nonce, then ciphertext plus tag.
  - `AppCore::start(dir, box_key, name)` errors with `"box key must be 32 bytes"` on any other length.
  - `CoreEvent::KeyReset` (mirrored as `AppEvent::KeyReset`), emitted once at start when the identity was replaced.
  - Dart: `Future<Uint8List> loadBoxKey()`.
    - Reads `wado.box.v1` from `FlutterSecureStorage`, or creates 32 random bytes and stores them.
    - Debug Linux only: honours `WADO_DEV_BOX`.
    - On Linux with no Secret Service and no dev box, throws `NoKeystore`.

- [ ] **Step 1: Failing tests:**
  - `aes_box_roundtrip`.
  - `aes_box_rejects_tamper` (flip one byte → `Err`).
  - `two_wraps_differ` (fresh nonces).
  - `lost_box_key_starts_fresh_and_says_so`: write an identity with key A, start with key B. Expect:
    - a new `client_key`
    - `identity.lost` exists and holds the old bytes
    - exactly one `KeyReset` event
    - a third start with B gives no `KeyReset` and the same key as the second start.
- [ ] **Step 2: Run** `cargo test -p wado-app-bridge -p wado-client-core`. Expected: FAIL.
- [ ] **Step 3: Implement.** In `identity.rs`, an unwrap failure renames to `identity.lost` and creates fresh. A read I/O error stays an error: it is not a reset.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Integration test** `identity_survives_restart`: start, read the key, `shutdown`, start again, same key. Also `release_ignores_dev_box`: a unit test on `loadBoxKey` with `kReleaseMode` injected. Then run both on Linux.
- [ ] **Step 6: Commit:** `app: box key in the platform keystore`.

---

## F1 — Home, pairing and On this phone (replaces the Compose P1 screens)

**Ships:**
- Pair a computer natively (QR, link or typed code with check words).
- Home with OS-logo cards, honest live link words, session counts and Running.
- A first real **Computer page**: hero, link, sessions (read-only) and Forget.
- On this phone: Appearance with Size, Diagnostics and About.
- Navigation restored on resume.

**Not yet:** doors for later phases show disabled, with "Coming to wado next — use wado meanwhile" and an "Open wado" button. That button launches the old app, so nothing is a dead end.

### Task F1.1: Theme — base16 roles, schemes, type, motion tokens

**Files:**
- Create: `app/lib/ui/theme/{base16,schemes,type,motion,theme}.dart`
- Create: `app/assets/schemes.json` (copied by `app-build.sh` from `crates/client/src/schemes.json`, never hand-edited)
- Create: `app/assets/fonts/` (Bricolage Grotesque and JetBrains Mono, OFL; licence files beside them)
- Test: `app/test/theme/contrast_test.dart`, `app/test/theme/schemes_test.dart`

**Interfaces:**
- Produces:
  - `class Roles { Color bg, surface1, surface2, line, text, dim, bright, accent, ok, warn, bad; }`, mixed from a scheme as `crates/client/src/theme.rs` does. Port the mixer; read it first.
  - `Roles rolesFor(Scheme s, {required int accentSlot})`.
  - `List<Scheme> loadSchemes()`.
  - `ThemeData wadoTheme(Roles r)`.
  - `abstract final class Motion { static const spring; static const pop; static const dur…; }`, with every DESIGN.md token, including `durHero 520`, `durPress 120`, `durStage 620`, `durStageOut 520` and the reduced set `120/200/160/120/0/160/160`.

- [ ] **Step 1: Failing tests:**
  - `contrast_test`: for three schemes (the default plus two from `schemes.json`, one light), `text` on `surface1` ≥ 4.5:1 and `dim` on `surface1` ≥ 3:1.
  - `schemes_test`: every scheme has 16 slots; the default scheme's `accent` equals the web's for the same slot. Pin one hex taken from `theme.rs`'s test.
- [ ] **Step 2: Run** `flutter test test/theme`. Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit:** `app: base16 theme, type and motion tokens`.

### Task F1.2: SizeScope and phone prefs

**Files:**
- Create: `app/lib/ui/theme/size_scope.dart`, `app/lib/ui/common/tap.dart`, `app/lib/prefs/prefs.dart`
- Test: `app/test/theme/size_scope_test.dart`, `app/test/theme/size_matrix_test.dart`

**Interfaces:**
- Produces:
  - `enum UiSize { compact(0.9), cozy(1.0), comfy(1.1), large(1.2) }`.
  - `class Look { scheme; accentSlot; corners; motion; double? text /* null = follow phone; 0.85–1.5 in 0.05 steps */; UiSize ui = UiSize.cozy; }`.
  - `class Prefs { static Future<Prefs> load(); ValueNotifier<Look> look; Future<void> setLook(Look); }`, backed by `shared_preferences` under `look.v1` as JSON.
  - `SizeScope({required Look look, required Widget child})`, as A7.
  - `Tap({required VoidCallback? onTap, required Widget child, String? semanticLabel})`, with a minimum size of `44 / ui`.

- [ ] **Step 1: Failing tests:**
  - `tap_is_44_on_glass_at_every_ui_size`: for each `UiSize`, the `Tap`'s rendered size × `ui` ≥ 44.
  - `text_follows_phone_when_null`.
  - `stage_is_never_scaled`: a widget outside `SizeScope` keeps its size when `ui = 1.2`.
  - `SizeMatrixTest`: home, a card, Pair and Appearance at 360×640, text 1.5 and `UiSize.large`. Collect `FlutterError`s; expect none ("A RenderFlex overflowed" fails). The screens arrive in later tasks, so this test grows: write it now with the home placeholder, and each screen task adds its screen.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit:** `app: Size model and phone prefs`.

### Task F1.3: Routes and restoration

**Files:**
- Create: `app/lib/nav/routes.dart`, `app/lib/nav/transitions.dart`
- Modify: `app/lib/main.dart`
- Test: `app/test/nav/routes_test.dart`, `app/integration_test/nav_restore_test.dart`

**Interfaces:**
- Produces:
  - `GoRouter wadoRouter(CoreHost core, Prefs prefs)` with A7's routes. Screens not built yet are a `Placeholder` with the route name.
  - `Page<void> wadoPage(Widget child, {required LocalKey key})`: opaque, using the M17 transition from `transitions.dart`, with predictive back on Android.
  - `void enterSession(BuildContext c, String key)`: **the only caller of `/c/:key/session`**. It is used in tap handlers only.

- [ ] **Step 1: Failing tests:**
  - `PagesAreOpaqueTest`: push `/phone/appearance` over `/`. The route is `opaque`, and no widget from `/` is painted (`find.byKey(homeKey).hitTestable()` finds nothing).
  - `session_route_only_from_enter_session`: grep-style test that reads `lib/` and asserts `'/session'` appears in `routes.dart` and in `enterSession` only.
  - `nav_restore_test` (integration, Android): go to `/phone/appearance`, `restartApp` via `RestorationManager`, and expect still on `/phone/appearance`.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit:** `app: routes, opaque pages, restoration`.

### Task F1.4: CoreHost — the event stream as screen models

**Files:**
- Modify: `app/lib/core/core_host.dart`
- Create: `app/lib/core/models.dart`
- Test: `app/test/core/core_host_test.dart`

**Interfaces:**
- Consumes: `AppCore.events`, `computers`, `watch`, `setVisible`, `pair*` and `forget`. Also `AppEvent`, `ComputerInfo` and `LinkInfo` (F0.2).
- Produces:
  - `class CardModel { key; name; osSlug; hue; LinkInfo? link; int sessions; bool playing; }`.
  - `class RunningModel { computerKey; computerName; firstApp; participants; }`.
  - `class HomeModel { List<CardModel> computers; List<RunningModel> running; bool keyReset; }`.
  - `class CoreHost extends ChangeNotifier`:
    - `HomeModel get home`
    - `void watchHome(bool visible)`
    - `Stream<Answer> answers`, where `Answer { int op; bool ok; String? why; bool noAnswer; }`
  - `CoreHost.forTest(Stream<AppEvent> events, FakeCore core)`: the seam. Every screen test uses it.

- [ ] **Step 1: Failing tests:**
  - `cards_follow_link_events`.
  - `resume_applies_only_newest_per_slot`: feed `Link(k, online)` then a backlog of `Link(k, reconnecting)` and `Link(k, online)`, with the stream paused and resumed. Final `online`, and `notifyListeners` was called at most once per resume batch.
  - `key_reset_sets_flag_once`.
  - `answer_maps_ok_fail_and_no_answer`.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.** Coalesce on a microtask: collect events, apply the newest per `slot()`, notify once.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit:** `app: CoreHost screen models`.

### Task F1.5: OS logos

**Files:**
- Modify: `scripts/os-logos.mjs` (an `--out app/assets/os` mode that writes `<slug>.svg`; the licence check and pin are unchanged)
- Create: `app/assets/os/*.svg`, `app/assets/licenses/os-logos.txt`, `app/lib/ui/common/os_disc.dart`
- Test: `app/test/ui/os_disc_test.dart`

**Interfaces:**
- Consumes: `osLogoSlugs()` (F0.2).
- Produces: `OsDisc({required String? slug, required Color hue, double size = 46})`, a monochrome logo in `Roles.bright`. A null or unknown slug shows the `linux` logo.

- [ ] **Step 1: Failing test:** `every_core_slug_has_an_asset`, which loads each `osLogoSlugs()` entry from the bundle. Also `unknown_slug_falls_back_to_linux`.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.** Run `node scripts/os-logos.mjs --out app/assets/os`.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit:** `app: OS logos`.

### Task F1.6: Home and the first Computer page

**Files:**
- Create: `app/lib/ui/home/{home_screen,computer_card,running_list,phone_section,add_computer}.dart`
- Create: `app/lib/ui/computer/{computer_screen,hero,sessions_readonly,coming_door}.dart`
- Create: `app/lib/ui/common/link_words.dart`
- Create: `app/lib/platform/open_old_app.dart` (+ `android.dart`, `none.dart`)
- Test: `app/test/ui/home_test.dart`, `app/test/ui/computer_test.dart`

**Interfaces:**
- Consumes: `CoreHost.home`, `watchHome`; `OsDisc`; `Tap`; `wadoPage`; `enterSession`.
- Produces:
  - `HomeScreen()` laid out as Compose A7 "Home" (wordmark, Computers, Running, On this phone, Add a computer) with the card spec there, including:
    - the 80-high card and 46 disc
    - Bricolage 17/700
    - the mono OS · compositor line
    - link words with tone motion
    - "· N sessions"
    - the 48 ▶, disabled while offline
  - `ComputerScreen(key)`: hero (disc, name, link words); "Sessions" read-only rows; "Forget this computer" with confirm through `Pending`; and coming doors (Desktop, Virtual input, Files, Shell) as `ComingDoor`.
  - `OpenOldApp.launch()`: Android only, via jnigen `PackageManager.getLaunchIntentForPackage("io.github.sandptel.wado")`; `none` hides the button.

- [ ] **Step 1: Failing tests** (with `CoreHost.forTest`):
  - `card_shows_logo_name_link_words_and_count`.
  - `play_disabled_offline`.
  - `tap_card_opens_computer_page`.
  - `watch_only_while_home_visible`: push a page over home, `watchHome(false)` called; pop, `watchHome(true)`.
  - `key_reset_banner_shows`.
  - `forget_asks_then_calls_core_and_returns_home`.
  - `coming_door_explains_and_offers_old_app`.
  - Add home and a card to `SizeMatrixTest`.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.** Watch on route visibility (`RouteAware` + `AppLifecycleListener`) → `watchHome`.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit:** `app: home and the first Computer page`.

### Task F1.7: Pairing

**Files:**
- Create: `app/lib/ui/pair/{pair_screen,pair_typed_screen,scan_view,check_words}.dart`
- Create: `app/lib/platform/links.dart` (`app_links`)
- Modify: `app/android/app/src/main/AndroidManifest.xml` (`wado://pair` and the `https` pairing-link intent filters, copied from `android/app/src/main/AndroidManifest.xml`)
- Test: `app/test/ui/pair_test.dart`, `app/integration_test/pair_deeplink_test.dart`

**Interfaces:**
- Consumes: `AppCore.pair`, `pairTyped`; `CoreHost.answers`; `AppEvent.pair(op, step)`, the mirror of `CoreEvent::Pair { op, step: PairStep }` (F0.2).
- Produces: `PairScreen({String? link})`, `PairTypedScreen()`, `Stream<Uri> incomingLinks()`.

- [ ] **Step 1: Failing tests:**
  - `typed_code_formats_as_xxxx_xxxx`.
  - `check_words_show_and_wait_for_computer`.
  - `refused_says_why_and_offers_try_again`.
  - `no_answer_shows_still_waiting_then_try_again`.
  - `PairDeepLinkTest.cold_and_warm_start_once` (integration): open the app with a `wado://pair?...` link cold, and again warm. Each time `pair` is called exactly once (count on the fake core), and a restore does not call it again.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
  - Scan with `flutter_zxing`'s `ReaderWidget` (camera permission asked on first use, with the reason shown first).
  - "Type the code instead" → `PairTypedScreen`.
  - The link is held in route state, and a pairing is started only when the restored state holds no finished `PairStep` for that link's op.
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Live check:** pair the emulator with the dev computer (`wado pair` shows the QR; the emulator scans its own virtual camera scene, or the typed code). The card goes online within the budget (Compose A8: ≤ 1.5 s p50 via the relay).
- [ ] **Step 6: Commit:** `app: native pairing`.

### Task F1.8: Pending — action feedback

**Files:**
- Create: `app/lib/ui/common/pending.dart`
- Test: `app/test/ui/pending_test.dart`

**Interfaces:**
- Consumes: `CoreHost.answers`.
- Produces:
  - `class Pending extends StatefulWidget { Pending({required int? Function() start, required Duration takingLong, required Widget Function(BuildContext, PendingState) builder}); }`.
  - `enum PendingPhase { idle, pressed, working, takingLong, ok, failed }`.

- [ ] **Step 1: Failing tests** (fake clock):
  - `pressed_under_100ms_then_working_by_300ms`.
  - `taking_long_after_threshold_offers_cancel_and_try_again`.
  - `ok_is_quiet`.
  - `failed_shows_reason_and_try_again`.
  - `never_frozen_a_second_tap_while_working_is_ignored_not_queued`.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit:** `app: Pending action feedback`.

**Execution order for F1:** F1.1–F1.5, then F1.8, then F1.6, F1.7, F1.9 and F1.10. F1.6 and F1.7 use `Pending`.

### Task F1.9: On this phone — Appearance, Diagnostics, About

**Files:**
- Create: `app/lib/ui/phone/{appearance_screen,size_controls,diagnostics_screen,about_screen}.dart`
- Create: `crates/app-bridge/src/api/diag.rs` (`fn diagnostics(&self) -> Vec<LinkDiag>`, port of what the Compose `DiagnosticsScreen` read through mobile-core)
- Test: `app/test/ui/phone_test.dart`

**Interfaces:**
- Consumes: `Prefs`, `SizeScope`, `UiSize`, `loadSchemes`.
- Produces: `AppearanceScreen`, with:
  - scheme, accent, corners and motion
  - **Text size**: Follow phone or 85–150 % in 5 % steps
  - **Interface size**: Compact / Cozy / Comfy / Large, default Cozy
  - a live sample that applies on release

  Also `DiagnosticsScreen` (per computer: link state, hops, relay caps, core version, `flutter_webrtc` version) and `AboutScreen` (AGPL, OS-logo licences, fonts, `LicenseRegistry`).

- [ ] **Step 1: Failing tests:**
  - `text_size_slider_steps_by_5_and_applies_on_release`.
  - `interface_default_is_cozy`.
  - `follow_phone_clears_text`.
  - `diagnostics_lists_each_computer`.
  - `about_lists_os_logo_licences`.
  - Add Appearance to `SizeMatrixTest`.
- [ ] **Step 2: Run.** Expected: FAIL.
- [ ] **Step 3: Implement.**
- [ ] **Step 4: Run.** Expected: PASS.
- [ ] **Step 5: Commit:** `app: On this phone`.

### Task F1.10: F1 close-out — live e2e, budgets, the APK

**Files:**
- Create: `scripts/app-e2e.sh`. It starts `target/release/wado-relay` and `target/release/wado daemon` with temporary `XDG_CONFIG_HOME`/`XDG_RUNTIME_DIR` (copy the setup from `scripts/core-e2e.mjs`), mints a pairing link, and runs `flutter test integration_test/live_test.dart -d <linux|emulator> --dart-define=WADO_E2E_LINK=<link>`.
- Create: `app/integration_test/live_test.dart`
- Modify: `docs/app.md`, `HANDOVER.md`, `CHANGELOG.md` ("Unreleased")
- Modify: `WADO_PLAN.md` Decision Log (A9.1–10 lines; gitignored, not committed)
- Modify: `plan/memory/latency/measurements.md`

- [ ] **Step 1: Write** `live_test.dart`:
  - `pairs_and_goes_online`: pair from the define, card online within 5 s.
  - `forget_removes_card`.
  - `daemon_restart_reconnects`: the script SIGTERMs the daemon by exact PID and relaunches it; the card shows Reconnecting, then online within 10 s.
- [ ] **Step 2: Run** `scripts/app-e2e.sh linux`, then `scripts/app-e2e.sh emulator`. Expected: PASS on both.
- [ ] **Step 3: Measure** on the user's phone and record in `plan/memory/latency/measurements.md`:
  - cold start → first frame (`--trace-startup`)
  - cold start → live link words (core log)
  - the home scroll frame p95
  - APK size (arm64)

  Report what the numbers say against Compose A8, including misses.
- [ ] **Step 4: Build** `scripts/app-build.sh --android --debug` → `~/Downloads/wado-next-0.0.5-debug.apk`.
- [ ] **Step 5: Commit:** `app: F1 live e2e and docs`. Push.

---

## F2–F9 — Outlines

Each phase gets **its own detailed plan** (`docs/superpowers/plans/<date>-app-f<N>.md`, the same format as F0/F1) written when it starts. It argues from this Part A and from Compose plan Part A.

For each phase:
- **Its daemon and core tasks are the Compose plan's P-phase tasks of the same number, unchanged.** They are framework-free.
- **Its UI tasks** are the Kotlin files there, mapped to `app/lib/ui/...` under the same names in snake_case.
- **Every phase ends like F1.10:** live e2e, measured budgets, the APK in `~/Downloads`, Decision Log lines, and a HANDOVER update.

### F2 — Computer page, settings, passkey (= Compose P2)
- **Core and daemon:** Compose P2 tasks 1–6 (control-room verbs, `config/{view,set}`, `host/actions`, `passkey/{msgs,flow}`, live host feed).
- **UI:** `ui/computer/{hero,meta_strip,hop_bar,actions,now_playing,quick}.dart`, `ui/common/{thick_slider,toggle_tile}.dart`, `ui/home/quick_lip.dart`, `ui/computer/sessions.dart` (verbs; **Join only from a tap → `enterSession`**), `ui/computer/pages/{display_session,sound_devices,connection_trust}.dart`, `apply_bar.dart`, `eyebrow.dart`, and `ui/passkey/{passkey_card,passkey_sheet}.dart`.
- **Platform:** `platform/passkey.dart` → `Authenticator.kt` through jnigen. Volume keys while the lip is open go through `platform/volume_keys.dart` (Android `KeyEvent` via a `FlutterActivity` override in `MainActivity.kt`, the one Kotlin addition).
- **Tests:** Compose P2's list, as widget tests plus `integration_test`. Add `HeroTransitionInterruptTest` (a second tap mid-Hero reverses).

### F3 — Virtual input (= Compose P3a + P3b)
- **Core and daemon:**
  - Compose P3 tasks: pad link on control rooms, holds and take over, `Button`, `SpeedHold`, the `wado_protocol::layout` model with six touchpad variants, `placePx`, two-finger synthesis.
  - **Media lands here:** `core.offer/ice` signalling for the pad PC, and `platform/rtc.dart` over `flutter_webrtc` with the ICE config of A4.
- **UI:**
  - `ui/input/{hub,touchpad,surface_painter,trails,gesture_track,keyboard,gamepad,layout_view,layout_editor,variant_picker}.dart`.
  - One `CustomPainter` for rings, trails and afterglow, with preallocated ring buffers (Compose A7 "surreal layer" and budget).
  - Raw `Listener` pointer events (not gesture recognisers) for the surface.
  - Haptics via `platform/haptics.dart` → `View.performHapticFeedback` constants named in Compose A7, through jnigen.
- **Gate R2:** event → `send` p90 ≤ 2 ms on the user's phone. If it misses, move the pad PC into the core (A4) before building the variants.
- **Tests:** Compose P3's list, plus `HintOverlapTest` and `pad_send_latency` (a logged measurement, not a unit test).

### F4 — Files and the viewer (= Compose P4)
- **Core and daemon:** Compose P4 tasks, plus `files/serve.rs` (the loopback range server; binds `127.0.0.1` only, a random port, and a per-stream random path token) and `#[frb(sync)] files_feed`.
- **UI:** `ui/files/{browser,row,ops_sheet,transfers,find}.dart` and `ui/viewer/{viewer,image,pdf,media,text,markdown,json,csv,archive,font,info}.dart` (A5 picks).
- **Platform:** the share target ("Send to computer") moves to `app/android` Kotlin, called through jnigen.
- **Tests:** Compose P4's list, plus `serve_rejects_wrong_token` and `serve_binds_loopback_only` (Rust).

### F5 — Shells (= Compose P5)
- **Core and daemon:** Compose P5 tasks.
- **UI:** `ui/shell/{list,terminal_page,extra_keys}.dart` on `xterm`.
- **Tests:** Compose P5's list, plus `ime_composition_reaches_pty` (integration, Android).

### F6 — The session (= Compose P6)
- **Core and daemon:** Compose P6 tasks (view room, `session/{intent,input,presence,stats}`, `playout-delay` switch).
- **UI:** `ui/session/{stage,console,cockpit,dock,control_centre,tiles,quick_rail,apps_sheet,stats_overlay}.dart`.
  - The stage is an `RTCVideoView` on Stage Black, revealed on the first frame.
  - The stage never takes IME insets (`resizeToAvoidBottomInset: false` on the session `Scaffold`; only the keys face pads).
- **Gate R1:** glass-to-glass p50 ≤ the WebView's, with the same phone, network and session, 20 samples each, 240 fps camera. If it misses, the PlatformView `SurfaceViewRenderer` path (A4) becomes the default on Android, with the Texture path kept as the switch.
- **Tests:** WebView defects 1–5 as integration tests (`ConsoleImeTest`, `StageFirstFrameTest`, `StageStatesTest`, `SessionBackTest`, core `late_start_is_detached`).

### F7 — Clipboard, push and live events, Claude, presence, the lane, widgets (= Compose P7 + the live-events addendum)
- **Core, daemon and relay:** Compose P7 tasks and the addendum (daemon `events/` with seq and replay, the relay `inbox` cap, `host_gone`).
- **UI:** `ui/clipboard/*`, `ui/claude/*`, `ui/home/widgets/*`, `ui/common/lane_hint.dart`, `ui/phone/background_screen.dart` (Stay connected, notification channels).
- **Platform (Android):**
  - `StayConnectedService.kt` becomes the `remoteMessaging` foreground service holding the core's links while backgrounded.
  - One notification channel group and one `MediaSession` per computer (`Notifier.kt`, a new `MediaSessions.kt`).
  - `ClipSync.kt` kept.
  - All through jnigen.
- **Tests:** Compose P7's list, plus the addendum's two-daemons-one-phone e2e on the emulator.

### F8 — iPhone and iPad, macOS, Linux as products
- **Apple build:** `scripts/app-build.sh --ios|--macos` builds `wado-app-bridge` as an xcframework (`aarch64-apple-ios`, `aarch64-apple-ios-sim`, `aarch64-apple-darwin`, `x86_64-apple-darwin`) and links it from the Runner. This needs a Mac with Xcode and runs outside Nix. Signing is the user's step.
- **Platform impls:** `platform/*/apple.dart` through ffigen/swiftgen:
  - haptics (`UIImpactFeedbackGenerator`)
  - passkeys (`ASAuthorization`)
  - MediaSession → `MPNowPlayingInfoCenter`
  - share extension
- **Open questions** for that phase's plan (ask the user then; do not decide now):
  1. The passkey origin and rpId on Apple. The Android app origin does not apply, and the daemon must learn the Apple origin, additively.
  2. iOS background. iOS suspends sockets, so live events while closed need APNs, a third party, against the user's "over wado's own link" wish. Options: APNs carrying only "wake and fetch over wado", or no background notifications on iOS.
- **Desktop session input:** a physical mouse and keyboard map to session input directly (no touch emulation). Pointer lock and keyboard capture via `platform/pointer_lock.dart`. The desktop window is the session at the window's size (invariant 8).
- **Linux:** a `.desktop` file, the tray via `platform/tray.dart`, and the software-decode ceiling measured and written in `docs/app.md`.
- **Tests:** boot, pair and live e2e on macOS and Linux; boot and pair on the iOS simulator.

### F9 — Cutover
- **Package:** the app takes `io.github.sandptel.wado` and is signed with the old app's key, so it installs *over* the old app as an update. The new app can't read the old app's pairings (they live in the WebView), so it shows "Pair your computers again" once.
- **Deletions:** `android/`, `crates/mobile-core`, `scripts/android-core.sh`'s mobile-core default, and the web bundle copy. Update `CLAUDE.md`'s pins ("Dioxus 0.7" stays for the browser client) and add the `flutter_rust_bridge` exact pin with its reason.
- **CI:** builds Linux and the Android APK, and runs `flutter test`, `flutter analyze` and `cargo test -p wado-app-bridge`.
- **Release notes** in `CHANGELOG.md`, and a tag.

---

## Order and size

| Phase | Size | Depends on |
|---|---|---|
| F0 | S: 4 tasks | — |
| F1 | M: 10 tasks | F0 |
| F2 | M | F1 |
| F3 | L (R2 gate first) | F1; F2 for holds UI |
| F4, F5 | M each | F1 (parallel to each other and to F3) |
| F6 | L (R1 gate first) | F3's rtc layer |
| F7 | L | F2 |
| F8 | L | F6 |
| F9 | S | F7 (Android cutover does not wait for F8) |
