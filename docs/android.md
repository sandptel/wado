# wado for Android

`android/` is a native Kotlin app (Jetpack Compose, minSdk 26, compile/target SDK 37) that hosts
the **same web client** the browser runs, bundled in the APK, in an Android System WebView, and
draws its own native screens over it. One client codebase: the app is a better host for it, not a
fork. It is AGPL-3.0-only like the rest of wado (About screen: licence text and source link).

## Shape

```
MainActivity ── native screens (Compose) ─────────────── Home · Pair (QR) · Share sheet · Settings · About
     │
     └─ WebView (one per process, WadoApp.web) ─ https://appassets.androidplatform.net/
            the bundled client (assets/client/, served by web/ClientServer.kt)
            window.wadoNative  ⇄  bridge/Hub.kt  ⇄  notify/ clip/ media/ push/ stay/ device/
```

- **The client is served from the APK** by `androidx.webkit.WebViewAssetLoader` on
  `https://appassets.androidplatform.net/` — a secure context, so WebRTC, WebCrypto and storage
  behave as on the real site. It changes only with an APK update: no remote code is loaded, and
  no service worker is registered (`js/app_sw.js` reports `state: "native"`), which also
  sidesteps the installed web app's trust-on-first-use worker caveat.
- **The WebView outlives activities** (`WadoApp`), because it holds the relay link: "Stay
  connected" keeps the process, and so the page and its link, alive with no screen up.
- **Native home first.** The app opens on its own home, drawn from the computers and scheme the
  page last reported (kept in `Prefs`, so it is instant on a cold start), with live status from
  the relay's `/online/:id`. Every door — Desktop, Trackpad, Files, Shell, a computer's page —
  opens the client at that computer (`nativeGo`, `crates/client/src/native.rs`). The client's
  own screens are the long tail until each is ported.
- **Safe areas**: the app is edge-to-edge; outside a session the WebView is padded by the system
  bar insets natively (so the page's `env(safe-area-inset-*)` reads 0 there), and while a session
  or the trackpad is up the window goes immersive and the page has the whole screen.

## The bridge: `window.wadoNative`

Injected with `WebViewCompat.addWebMessageListener` for the app's own origin only, so a page
navigated elsewhere never gets it. The page feature-detects it (`js/native.js`: `W.native` is
null in a browser, and the `native_*.js` files then do nothing). Messages are JSON with a `t`.
`window.wadoNativeInfo` = `{platform, version, sdk}` is set before the page runs.

| `t` | direction | fields | what |
|---|---|---|---|
| `theme` | page → app | `hexes` (16 × rrggbb) | the scheme in use; native screens follow it |
| `computers` | page → app | `profiles[{key,name,host,relay_url,remote_id,conn_mode}]`, `selected` | for native home, share sheet, shortcuts. Never a key or a pin |
| `nowPlaying` | page → app | `bus,app,title,artist,art,playing,position_ms,length_ms,can_*,volume,muted,sink` or `{}` | the computer's MPRIS player → a media session |
| `clip` | both | page → app `{text, sensitive}` the computer copied; app → page `{text}` the phone's copy | clipboard sync |
| `haptic` | page → app | `kind`: tick, tap, claim, confirm, reject, heavy | predefined effects; `navigator.vibrate` is mapped onto these |
| `active` | page → app | `on` | a session or the trackpad is up: keep the screen on, go immersive |
| `orient` | page → app | `lock`: landscape, reverse, none | real orientation lock for the trackpad's landscape (no fullscreen) |
| `notify` / `notifyClear` | page → app | `kind,id,title,body,actions[{id,label}]` | a system notification while wado is not in front |
| `notifyAction` | app → page | `kind,id,action` | a notification button; `W.nativeActions[kind](id, action)` answers it |
| `pushSubscribe` | page → app | `vapid` | register with UnifiedPush; answered by `pushEndpoint {endpoint, keys:{p256dh,auth}}` or `{error}` |
| `go` | app → page | `key, go` | open this computer (and a door: trackpad, files, shell, session) |
| `back` | app → page | — | system Back with no page history left; the page answers `exit` at its home |
| `share` | app → page | `key, files[{url,name,mime,size}], links[], text` | "Send to computer"; answered by `shareDone {ok, why}` |
| `media` / `volume` | app → page | `op[, ms]` / `volume` | lock-screen / Bluetooth controls, volume keys |

## What the native side adds

- **Orientation, immersive, screen on** (`device/ScreenModes.kt`): the trackpad's landscape asks
  for a real lock instead of fullscreen + drawing turned.
- **Haptics** (`device/Haptics.kt`): `HapticFeedbackConstants` (CONFIRM, REJECT, GESTURE_START,
  …), so the system's "touch feedback" switch is respected. Pad taps tick, gesture claims and
  action confirmations/refusals each have their own feel.
- **Passkeys**: `WebSettingsCompat.setWebAuthenticationSupport(…FOR_APP)` is on. For a passkey
  to work the relying party must be associated with the app by Digital Asset Links — the RP ID
  is the page's origin, `appassets.androidplatform.net`, which no one can host
  `assetlinks.json` on. Passkeys therefore need the RP moved to a domain wado controls (with
  `/.well-known/assetlinks.json` naming this app's signing key) before they work in the app;
  until then the page sees no authenticator, as in any browser without one.
- **Share target** (`share/`): files, photos, links and text from any app → pick a computer →
  files upload into its `~/Downloads` through the file link (the page fetches each from
  `/share/<token>/<n>`, served by the app straight from the other app's content), links open in
  its session (`xdg-open`; needs a running session), plain text goes to its clipboard.
- **Shortcuts**: static Trackpad / Files / Shell (`res/xml/shortcuts.xml`, `wado://go/<door>`)
  on the last used computer, and a dynamic "Desktop on <name>".
- **QR pairing** (`ui/pair/`): CameraX + ZXing core (Apache-2.0, pure Java; no Google Play
  services). The code's link (`?relay&id&pair&hk`) is loaded onto the app's origin, where
  `js/storage.js` takes it in exactly as a browser would. "Type a code instead" opens the
  client's own form.
- **Hardware keyboards** reach the page as ordinary key events. **Gamepads**: the WebView's
  Chromium exposes the Gamepad API; not yet verified on a device (no InputDevice bridge built).
- **Notifications** (`notify/`): channels per urgency (Computer needs you · Finished · Apps ·
  Status · Playing · Staying connected). Shown while wado is not in front; Allow/Deny buttons go
  back to the page. Wired today: the session's app notifications. The F3 kinds
  (`claude_permission`, `claude_done`, `computer_online/offline`, `transfer_done`) arrive through
  `W.native.notify(kind, …)` once F3's page code calls it.
- **Background without Google**: "Stay connected" (off by default, `stay/`) — a
  `remoteMessaging` foreground service with a permanent, minimum-priority notification keeps the
  page's control link open. **UnifiedPush** (`push/`): with a distributor installed (ntfy, …),
  `W.native.pushSubscribe(vapid)` returns a Web-Push-shaped subscription the computer's RFC 8291
  sender can use unchanged. The installed web app in Chrome gets true Web Push instead.
- **Now playing + volume** (`media/RemoteMedia.kt`): the computer's player as this phone's media
  session (lock screen, shade, Bluetooth buttons); volume keys turn the computer's default output
  up and down while wado is in front, with an on-screen level. "This phone" audio is the page's
  own WebRTC audio, unchanged.
- **Clipboard** (`clip/ClipSync.kt`): the computer's copies land on the phone at once, marked
  `EXTRA_IS_SENSITIVE` when the computer says so; the phone's copy goes to the computer when wado
  comes to the front (Android allows no other read), never one marked sensitive (password
  managers mark theirs) and never one wado put there itself. The history view is the client's.

## Build

```sh
nix develop .#android -c scripts/android-build.sh          # client + debug + release APKs
nix develop .#android -c scripts/android-build.sh --debug  # debug only
```

The `android` shell (flake.nix) brings the SDK (platform 37.0, build-tools 37), JDK 17, and the
web client's toolchain (`dx`, cargo/rustc with wasm32, `wasm-opt`). The SDK is unfree; the shell accepts its licence
(`android_sdk.accept_license`) for itself only. AGP fetches `aapt2` as a generic-Linux binary
NixOS cannot run, so the shell points `android.aapt2FromMavenOverride` at the SDK's.

`nix develop .#android-emulator` adds the emulator and an x86_64 API 37 image (KVM).

Release signing: `docs/android-release.md`. CI builds the debug APK (`.github/workflows/android.yml`).
