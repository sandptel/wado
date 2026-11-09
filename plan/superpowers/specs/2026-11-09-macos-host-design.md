# macOS host, without the compositor (design, 2026-11-09)

Decided with the user in a grilling session, 2026-11-09. Recon:
`plan/superpowers/research/2026-11-09-macos-host-recon.md`. Cross-platform plan this is a slice of:
the user's "xprojects" page (X1 + parts of X2/X4, minus video).

## Intent

Get the wado daemon running on this Mac and useful from the phone **before** any video work.
The session/compositor part (`VideoBackend` on macOS) is deliberately out — it needs careful design
of its own. Everything else that can be tested on a Mac now ships in one run.

A Mac is **never** mistaken for a Linux host by any client: it announces itself, and the client UI
has a macOS variant.

## In scope (user decisions)

| Area | Decision |
|---|---|
| Input | Full Mac feel: trackpad mode interpreted on the host (move, tap-click, two-finger tap = right click, tap-drag, two-finger scroll with scroll phases + momentum, three-finger swipes → Mission Control / App Exposé / Spaces via the Mac's own symbolic hotkeys, pinch → ⌘+/⌘−), keys + chords (evdev → kVK), typed text (Unicode), gamepad "keys" mode |
| Files | Full parity: browse, transfer, rename/move/new folder, Trash via the Mac's Trash (Put Back works), QuickLook thumbnails, the same folder sandbox re-done with macOS-safe calls (`O_NOFOLLOW_ANY`, `renameatx_np`) |
| Clipboard | Text + images both ways (NSPasteboard), wado-kept history on the Mac, concealed items never copied |
| Media | Media keys for control (any app); now-playing card for Music and Spotify via AppleScript (Automation prompt once each); other apps: controls only, card says so |
| Volume | Master level + mute + output device switching (CoreAudio); per-app streams Not on macOS |
| Brightness | Private DisplayServices (exact level, built-in display), resolved at runtime; fallback brightness keys; external monitors Not on macOS for now |
| Extras | Shells + Claude cards; keep awake (IOPM), lock, sleep; system info card (Apple logo, macOS version, chip, memory, uptime); launch apps (NSWorkspace) + screenshot (needs Screen Recording) |

## Out of scope this run → "Coming to macOS"

Session / video / picture input, sound from the Mac, windows list + actions, notifications mirror,
menus. **Not on macOS** (with reason): real gamepad (needs an Apple-granted DriverKit
entitlement), phone as the Mac's speaker (needs a virtual audio driver), per-app volume, Wi-Fi /
Bluetooth toggles, Hyprland touchpad settings, external-monitor brightness (for now), app isolation.

## Structure (user decision: separate builds, shared traits)

- Shared seam: `wado-platform` traits + `Cap`/`CapSet` registry (exist). A provider registers only
  what it has; the rest is honestly absent.
- macOS code lives in its own crate column (`crates/macos/host`, crate name my call), compiled only
  on macOS via `[target.'cfg(target_os = "macos")'.dependencies]`. Linux providers stay Linux-only.
- Server: Linux-only dependencies (`wado-compositor`, zbus, Linux libc calls, `pw-record`) behind
  `cfg(target_os = "linux")`. On macOS the server builds **without** the compositor; session routes
  answer "not on this computer yet". Linux behaviour unchanged.
- Language: Rust + objc2 crates; private frameworks via `dlopen`/runtime lookup, failing to "absent"
  rather than crashing (invariant 6: opening is the probe).

## Identity + capabilities on the wire (my call on shape)

- The daemon sends its **platform** (`macos` / `linux`) and its **CapSet with three states**:
  on / off-here (reason) / coming (planned on this OS). Today the CapSet is probed but never sent;
  it goes out with the config/host state. Old clients ignore unknown fields; a new client meeting an
  old daemon assumes Linux.
- `SysInfo.os_id = "macos"` picks the Apple logo.

## Client UI variant (user decision: two labels, both clients)

- Android app (Flutter) **and** web client (Dioxus) read platform + capability states.
- **Coming to macOS**: dimmed, tappable, says what is planned.
- **Not on macOS**: hidden from main surfaces, listed once in an "On this Mac" card with the reason.
- Missing permission (Accessibility, Automation, Screen Recording, a Files folder) is an off-here
  reason naming the exact System Settings pane.

## Running it (user decisions)

- Signing: self-signed `wado-dev` code-signing cert in the login keychain; daemon wrapped in a
  minimal `Wado.app` (LSUIElement) so TCC grants survive rebuilds.
- Startup: `wado daemon` / `wado tui` as on Linux. `wado install` LaunchAgent exists but is
  **optional and not enabled during testing** (the user has a later use for it).
- Network: the public Azure relay (`wado-relay.indiasouthcentral.cloudapp.azure.com`, up, git
  33f7532 on 2026-11-09).

## Process (user decisions)

- Work on branch `macos`; main untouched until the user says merge.
- GitHub Actions: Linux job (cargo check + test, the guard for not breaking Linux — it cannot be
  built here) and a macOS job.
- Pre-approved: install dev tools (Homebrew, Flutter, Android SDK/NDK, cargo-ndk, dx), create the
  signing cert, push `macos` + CI, run the daemon on the public relay. Anything else: ask.
- Ping only for permission grants and blockers. Done = CI green on both OSes, Wado.app running here,
  APK in `~/Downloads`, web client built, then a phone checklist for the user.

## Left to the builder

Crate names and layout inside the macOS column; the exact wire field names; gesture thresholds
(start from the Linux touchpad feel and Sill's numbers); the evdev → kVK table; how the web client
gets tested (local `dx serve` reachable from the phone, or a Pages preview — main must not deploy).
