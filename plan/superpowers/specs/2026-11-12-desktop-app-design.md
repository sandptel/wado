# wado desktop app — design (`2026-11-12`)

Crate `crates/desktop` (`wado-desktop`), Rust on gpui. Started on the Mac, branch `macos`.
Boards (the approved look, one file per screen): `plan/superpowers/specs/desktop-app/boards/*.dc.html`
(open in a browser; published copy: https://claude.ai/artifact/21w4puMZAcQGqWWstKhuwR, private).
Read `Language.dc.html` first, then the screen you are building.

## Why

One app a person runs instead of a terminal: it serves this computer (the daemon, wrapped) and
reaches other wado computers. The TUI's power, shown visually, for a larger audience.

## Two halves

- **This computer** (host control room): Overview, Devices, Settings, Health. TUI parity plus a
  settings editor, setup/health, live connections and the shared widgets (playing, clipboard,
  files moving, activity).
- **Computers** (client): list, add a computer, a page per computer with **Home / Shell / Input**.
  Home is a 12-column grid of widgets the person can move, resize, hide, add; layout kept per
  computer. Dedicated All apps and Files pages. Sessions open in their own windows, several at
  once (VS Code style).

## Decided (the user's answers)

| | |
|---|---|
| Daemon | Join a running one, else start one. Closing the window keeps serving (menu bar / tray). |
| Client order | Control first (apps, files, clipboard, controls), picture later. Trackpad is a tab, never the front page. |
| Platforms | macOS now; Linux marked (see "Linux targets"). |
| Toolkit | `gpui-kit` `=0.9.1` (gpui + gpui-component + Lucide), restyled with wado tokens. Apache-2.0, fine beside AGPL. |
| Look | wado `DESIGN.md` merged toward a native desktop feel. |
| Slint experiment | `origin/experimental/desktop-app` — mined (fonts, `caps` seam), then retired. |
| Kept on the device | Clipboard items and notifications received are stored locally and readable offline; written once in `wado-client-core` so the phone gets it too. |

## Architecture

```
desktop::shell   window, sidebar, theme, menu bar          (gpui)
desktop::host    Overview, Devices, Settings, Health       (gpui)
desktop::client  Computers, a computer page, sessions      (gpui)
        │  plain data, plain functions
desktop::caps    the ONLY module calling `wado`, `wado-config`, `wado-client-core`
        │
wado (server lib) · wado-config · wado-client-core · wado-platform · wado-ux
```

- `caps` reads what the TUI reads (`wado::cli::status`, `wado::gate`, the command sockets). A
  missing capability is data with a reason (`Cap::On | Cap::Off(why)`), never a hidden control.
- State is pushed: a background reader publishes snapshots; views re-render on change. The UI
  thread never blocks on a socket or the relay probe.
- gpui's executor and the daemon's tokio runtime meet only through channels.
- One job per file. No Smithay type above `caps`.

## Design language (tokens live in `crates/desktop/src/theme.rs`)

- Colour, base16 default-dark: ground `#181818`, sidebar `#1d1d1d`, card `#222222`, line/surface2
  `#2e2e2e`, bright `#ececec`, text `#c8c8c8`, dim `#9a9a9a`, accent `#7cafc2`, good `#a1b56c`,
  caution `#f7ca88`, warn `#dc9656`, bad `#ab4642` (bad text `#e08783`), mauve `#ba8baf` (client).
- Type: Bricolage Grotesque (titles), Figtree (body 13px), JetBrains Mono (ids, numbers). Fonts are
  in `crates/desktop/fonts/` (OFL).
- Sizes: controls 32 on a page, 28 in a card; switch 36×22; radius 14 card / 9 control / 8 small.
- Motion: settle `cubic-bezier(.22,1,.36,1)`, pop `cubic-bezier(.34,1.56,.64,1)`; 120 / 220 / 360 ms.
- Rule from the phone app: a widget never disappears for a missing permission or a lost link — it
  stays and says what is missing and what to do (`States.dc.html`, 24 states).

## Stages

| # | Stage | Done when |
|---|---|---|
| 0 | Barebones: window, sidebar, tokens, Overview on a real daemon, join-or-start | you open it and see your Remote ID and who is connected |
| 1 | Devices and pairing (grant tiles, QR, check words, "let devices in") | a phone pairs from the app and grants change live |
| 2 | Menu bar: close keeps serving, status, Pair, Quit, start at login | window closed, phone still works |
| 3 | Settings (forms over config.kdl, writes `ui.kdl`, Apply) and Health | a change here is the one the phone sees |
| 4 | Shared right now: playing, clipboard, files moving, activity | a copy on the phone shows here within a second |
| 5 | Client: Computers, add, computer page (Home widgets / Shell / Input), All apps, Files | this Mac opens an app and moves a file on another computer |
| 6 | Client: session windows (receive, decode H.264, draw; input on its own channel) | two sessions side by side |
| 7 | Ship: inside Wado.app, signed; Linux package + desktop entry | installs without a terminal |

## Linux targets (not built on the Mac — pick up here on the Linux machine)

Each is drawn on the boards with a `LINUX` tag. Build against the same tokens and widgets.

- [ ] **L1 Sessions and screens** — Overview "Connected now" rows for sessions at their real shape; sessions in use / allowed.
- [ ] **L2 Encoder tier** — chip on Overview and in each session window; software-encoding notice never dropped (invariant 5).
- [ ] **L3 Join and take over** — two more grant tiles on Devices (`join`, `takeover`).
- [ ] **L4 Settings sections** — Stream, Sessions, Sound, Gamepad, Window rules: forms exist, Linux turns them on.
- [ ] **L5 Health rows** — `/dev/uinput`, VA-API, render node, Wi-Fi/Bluetooth, gamepad — offered only when opening works (invariant 6).
- [ ] **L6 Session window extras** — workspaces rail, focused-window strip, app menus.
- [ ] **L7 Per-app volume, sound to this computer** — in Playing and sound.
- [ ] **L8 Game mode** — on an app in All apps and in the session window.
- [ ] **L9 Apps on their own screen** — open, hide, quit without a session.
- [ ] **L10 Tray** — StatusNotifierItem in place of the macOS menu bar item.
- [ ] **L11 Client decode** — VA-API decode for session windows.
- [ ] **L12 Build check** — `cargo run -p wado-desktop` on Linux (gpui Wayland/X11); `caps::daemon::hostname` and start path there.

## Open, taken as defaults until the user says otherwise (my calls, not theirs)

- Sidebar has four host pages; sessions live inside Overview.
- Default Overview widgets: Now playing, Clipboard, Files moving, Activity.
- Dark only at launch.
- Stage order as above (session windows after the computer page).
- Local store: last 200 clipboard items and 200 notifications per computer, 7 days, text and small images, encrypted at rest, cleared by Forget.
- Clipboard contents shown, not blurred.
- "Send every key": which key stays local is undecided — ask before stage 6.

## Risks

- gpui moves fast: one exact pin; time a clean build.
- Menu bar: if gpui has no status item, a small native one (objc2) sits beside it.
- Two daemons: the app and the login item both starting one — the join check must win.
- A Mac host does not send notifications yet; the card says why.
- Direct-mode HTTP control stays on localhost; the app never exposes it.
