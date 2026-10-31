# The app (Flutter) — `app/`

The native wado app for Android (first), Linux, and later iOS, iPadOS and macOS: Flutter screens
over wado's Rust client core. It replaces the Android app's WebView. Plan:
`docs/superpowers/plans/2026-10-31-flutter-app.md`; decision: `WADO_PLAN.md` Decision Log
2026-10-31 "One UI framework for every platform: Flutter".

## Shape

- `crates/client-core` — the protocol, written once in Rust: relay link, end-to-end handshake and
  seal, pairing, reconnect, and **rooms the UI opens itself** (`Core::open_room`: the seat, a control
  room or a view room), whose messages reach the UI raw. The app speaks the web client's protocol
  over them, 1:1 with `crates/client/src/js/*`.
- `crates/app-bridge` — what Dart sees (`flutter_rust_bridge`): `AppCore` (start, rooms, pairing,
  events as JSON) and the WebAuthn bytes for the passkey. One core per process.
- `app/lib/wado/*` — the ports of the web client's JS modules (relay_link, session, host, pad,
  files, shells, clip, passkey…). `app/lib/ui/*` — the ports of its Dioxus screens and CSS.
- `app/android/…/kotlin` — the Flutter host plus the Keystore passkey authenticator
  (`wado/passkey` method channel).

## Pins

- **`flutter_rust_bridge` 2.13.0 exactly** — the crate, the Dart package and the codegen
  (`flutter_rust_bridge_codegen` from the flake) must be equal, or the app refuses to start
  ("Content hash … different").
- Flutter 3.47.0 from the flake's nixpkgs. Android: compileSdk/targetSdk 37, NDK 29, build-tools
  37 and CMake 3.22.1 from the Nix SDK — `app/android/build.gradle.kts` forces every plugin onto
  them (the Nix SDK is read-only).
- Never use `pin!` in `crates/app-bridge`: it expands to `super let`, which the codegen's parser
  rejects.

## Build and run

```sh
nix develop .#app -c flutter_rust_bridge_codegen generate      # after changing crates/app-bridge/src/api
nix develop .#app -c scripts/app-build.sh --android [--release] # → ~/Downloads/wado-next-<v>-<mode>.apk
nix develop .#app -c scripts/app-build.sh --linux               # → app/build/linux/x64/debug/bundle/wado
LD_LIBRARY_PATH=$WADO_APP_LIBS app/build/linux/x64/debug/bundle/wado   # inside the .#app shell
```

On Linux the box key lives in the Secret Service; a debug build may use a key file instead:
`--dart-define=WADO_DEV_BOX=<path>`. libwebrtc needs libpulse at run time (`WADO_APP_LIBS`).

The app installs beside the old one as **`io.github.sandptel.wado.next`** ("wado next"), and is a
new device: pair each computer once (`wado pair` on the computer, then scan its QR).

## Testing without a person

Debug-only hooks (`app/lib/wado/e2e.dart`): `--dart-define=WADO_E2E_LINK=<pairing link>` pairs at
start; `WADO_E2E_SESSION=true` starts a session; `WADO_E2E_PAGE=<page>` opens a page. Pair it to a
**throwaway** relay and release daemon with their own `XDG_CONFIG_HOME` (never `~/.config/wado`),
as `scripts/core-e2e.mjs` does. On the emulator, reach the host's relay with `adb reverse
tcp:<port> tcp:<port>` and a `127.0.0.1` relay URL (10.0.2.2 does not answer the app).
