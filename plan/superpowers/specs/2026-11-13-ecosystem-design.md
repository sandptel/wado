# Ecosystem: the phone and the computer, both ways — design

2026-11-13. Asked for: the desktop app is mostly a *daemon runner*. Its home must show who is
connected, what each device is offered and using, how good its link is, and what the device
itself shares — "KDE Connect"-style, both directions. Run autonomously, end to end: protocol,
daemon, desktop, phone app.

## Root cause of "home is empty while my phone is connected"

The daemon learns a device is connected only from `RelayMsg::DeviceStatus` (writes
`~/.config/wado/links/`). Only the web client sends it. The Flutter app (seat via `RelayLink`,
other computers via `SideLinks`/control rooms) never does, and control rooms are not recorded at
all. So `Live::connected()` is 0 and the Overview shows the first-run guide.

Fix on both ends: the daemon records **every** link it holds for a trusted device (seat and
control rooms) as presence, and the phone reports its link and itself.

## Ideation — what an ecosystem shares

| Feature | Phone → computer | Computer → phone | Phase |
|---|---|---|---|
| Presence + link quality (direct/relayed, lan/internet, RTT, Wi-Fi/cell) | ✓ report every 5 s | — | 1 |
| Device facts: model, Android version, battery %, charging | ✓ | — | 1 |
| What each device uses now: session (size, fps, kb/s), widgets, shells | daemon knows | — | 1 |
| What each device is offered: its grants | daemon knows | — | 1 |
| Find my phone: ring it | — | ✓ ring | 2 |
| Find my computer: ring it | ✓ ring | — | 2 |
| Send text / link | ✓ (share sheet → inbox on desktop) | ✓ (desktop → notification on phone, copy/open) | 2 |
| Send this computer's clipboard to a phone | — | ✓ | 2 |
| Phone's notifications on the computer | ✓ opt-in, Notification access | — | 3 |
| Phone's now playing, controlled from the computer | ✓ opt-in, same access | ✓ play/pause/next/prev | 3 |
| Computer's now playing, clipboard, files (exists) | — | ✓ | — |

Left for later (said in the hand-off): SMS/calls, phone files browsed from the computer,
presentation remote, run-commands, battery-low alert on the computer, desktop notifications
dismissed on the phone.

## Protocol (`crates/protocol/src/peer.rs`, `RelayMsg` variants)

Device → computer, on the seat **and** on a control room (policy: any trusted device):
- `device_status {link}` — exists; now also accepted on control rooms.
- `device_info {info: {model, os, battery?: 0..100, charging}}`.
- `device_notes {notes: [{id, app, title, body, at_ms}]}` — a snapshot, newest first, ≤ 20.
- `device_playing {playing?: {app, title, artist, playing}}`.
- `ring {from}` — ring the computer.
- `share_text {from, text}` — text or a link for the computer's inbox.

Computer → device (sent down whichever link the device holds):
- `ring {from}`, `share_text {from, text}`, `device_media {op}` (`play-pause|next|previous`).

Names never start with `peer_` or `ctl` (the relay intercepts those).

## Daemon

- `crate::peers` (one job per file): `store.rs` — one JSON file per device in
  `~/.config/wado/peers/`, merged on each report, `at_ms` heartbeat; `route.rs` — this process's
  writers per device key (seat or control room), and a 5 s ticker that refreshes `at_ms` for every
  device with a live writer; `inbox`/`ring` fields for the desktop.
- Local socket: `peer <key> ring | text <text> | media <op>` — answered by the daemon holding a
  link to the device, `error: not here` from the others (the app asks each daemon of the pool).

## Desktop Overview (host-centric)

1. Header + chips (as now).
2. **Connected** — one card per connected device: name, model · OS, battery; link line with a
   quality dot; *Using now* (session size/fps/kb/s, widgets, …); *Offered* grant chips; its
   now playing with controls; its latest notifications; actions Ring · Send clipboard · Send text.
3. **Shared by this computer** — now playing ("shared with N devices"), clipboard, files moving.
4. Received (inbox from devices) and Activity.
5. Offline trusted devices: compact rows with last seen.
The guide stays for "nothing connected" (the person's earlier ask); it now lists the trusted
devices with last seen above the guide.
A ring from a device brings the window forward with a banner and a sound.

## Phone app

- `device_status` every 5 s from `Link.I.now` (seat) and from side links.
- `device_info` from a native `deviceInfo` call (Build.MODEL, release, BatteryManager).
- Handles `ring` (alarm sound + vibrate + notification with Stop), `share_text` (notification with
  Copy/Open), `device_media`.
- Opt-in "Share this phone's notifications and music" (NotificationListenerService), off by default.
- Computer page: Ring the computer; send text.

## Build/verify

Rust tests per crate; desktop screenshots (`shot.sh`); `flutter analyze` + APK build with
`JAVA_HOME=/opt/homebrew/opt/openjdk@17`, Flutter `~/sdk/flutter`, SDK `~/Library/Android/sdk`.
