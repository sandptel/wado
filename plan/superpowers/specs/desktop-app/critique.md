# Desktop app — self-critique log

After each stage the pages were drawn offscreen (`crates/desktop/shot.sh out.png <Page>`),
compared with the boards, criticised, fixed, and drawn again. Newest stage last.

## Stage 0 — Overview (v1 → v2)
- Connected now listed all 5 trusted devices as "not connected": noise. → Only connected ones;
  empty state says how many are trusted, with a Pair button that opens Devices.
- Relay chip printed the whole relay hostname. → "Relay connected".
- Under-the-header note line for actions. → A toast at the foot of the window, 4 s, red when it failed.

## Stage 1 — Devices (v1 → v2)
- Primary buttons rendered white: the kit's `button_primary*` were not themed. → Themed buttons,
  danger, switch thumb, popovers, selection in `theme.rs`.
- Five "Android · app" rows could not be told apart. → `key 1a2234` under each name.
- "TRUST ENDS" header wider than its column. → "ENDS".
- Owner's fixed tiles at 55 % opacity read as broken. → A muted teal fill (on, not yours to change);
  the tooltip says why.
- Open: "Let devices in" (pause every device) is drawn on the boards but the daemon has no pause;
  left out until it does.

## Stage 2 — Menu bar panel (v1 → v3)
- Fixed 420 px height left a dead third. → Height computed from what it shows.
- First fix clipped Quit and squeezed the dividers. → Dividers don't shrink; base height raised.
- Remote ID was text only. → Click copies it.
- Open: "Quit — stops serving": with the login agent's KeepAlive a killed daemon comes back, so
  Quit says "this computer keeps serving" and stopping is left to Health/launchctl.

## Stage 4 — Shared right now (v1 → v2)
- Left column twice the right. → Activity left, Files moving right under Clipboard.
- File rows read "download · 18 KB by Android": backwards for the person at this computer.
  → "sent to Android · app · 18 KB", "received from …", "deleted by …".
- Activity was empty on a Mac: Wado.app logs to /tmp/wado-rig/mac.log. → Read there too.
