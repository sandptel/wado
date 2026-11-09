# Player home, Game mode, many computers, delegated pairing — design

Status: **direction taken `2026-11-04`, nothing built.** Storyboard (private artifact):
https://claude.ai/artifact/9ujaw3YDYCMQbgwLFVoD3z. Decided with the user by multiple choice; the
Decision Log entry of the same date is the record. Client is the Flutter app (`app/`).

## Why

The client works for its developer and nobody else. Measured on the current app (IA audit,
`2026-11-03`): Steam is 10–12 taps (X server buried in Display & session, restart-only);
replaying a game is 4; full screen hides the gamepad; the desktop opens empty; picture gestures
are taught nowhere; a locked feature answers with a CLI command, a tooltip or silence; first run
assumes a terminal.

## Part 1 — the player (client-only)

| Decision | Detail |
|---|---|
| Persona | The player: paired once, non-technical. |
| Home | **Library first**, adaptive: recent gaming (last 2 sessions or 3 days) → a Steam-style carousel *Jump back in*; otherwise one Continue card for the last thing done (shell, file, desktop). Tabs: Home · Games · Apps. |
| Posters | The app's icon on a wash from its dominant colour **snapped to base08–0F** (Sixteen Slots Rule holds). No fetched art. |
| Game mode | **A per-app switch.** Turning it on asks once — how you play (pad / controller / touch + layout), then the session rules — and every later open *from the phone* uses them. The Games shelf is simply every app with Game mode on. Suggested after a pad session ("You played Lutris with the gamepad"). |
| Game mode rules, now | Settings the client already sets per session: Picture preset (Smoothest / Balanced / Sharpest / Custom, each showing its exact frame rate · tuning · lock · keyframes), redundant input + low-latency audio, client gestures off, volume keys + wake lock, X server for launchers that need it. |
| Game mode rules, later | Compositor: borderless full screen at client size, focus pin, no pop-ups, idle inhibit, pointer lock, frame pacing. Separate discussion. |
| Signature | The poster grows into the game; the four hops told as plain steps; a failed hop names itself and its fix. |
| In game | Full screen **with** the pad overlaid (the unused `pad_on`/`pad_opacity` settings). Corner pill: link dot · ⋯ · ?. Android back opens the menu. Layout picker inside the session. |
| Hints | All four: just-in-time once-only bubbles; explain-locked sheet (what · why · fix button · Details); ? overlay labelling every control and the gestures available here; 4-card tour after pairing. |

## Part 2 — many computers, one phone

| Decision | Detail |
|---|---|
| Top bar | Computer dropdown top-left (*All computers* first). **⋯** right = that computer's settings and connection; it always names the computer it acts on. **Scan** replaces ?; ? moves into ⋯ on home and stays in the game pill. |
| All-computers home | Today's widgets promoted to the home page; every card carries its computer's hue dot. Defaults come from each computer's grants. |
| Clipboard on home | **Shown in full** — the user's call against the recommendation (see accepted risks). |
| Handoff | **Pull only.** The other device's home shows "Hades, paused on Desk PC — Continue here" (sessions list + `takeover`). No push, no device list shared. |
| Pause | **Leave running is enough.** No process freeze in v1. |
| Scanner | Reads any wado code, always says what it is before acting. **A computer added by scan starts with clipboard sharing off**, with "This is a new computer. Only add computers you own." |
| `wado pair` | **Ask when run bare** in a terminal: the grant checklist (defaults `security { new-device }`) and an expiry, then the QR. Grant args, `--for <dur>` and `--yes` skip it; non-TTY keeps today's behaviour. TUI Pair gets the expiry too. |
| Expiry | **Built, enforced by the computer.** A trust-line end-time column; absent = forever, so old lines are unchanged. Expired lines are refused and pruned. |
| Delegated pairing | **The managing phone asks the computer to mint an ordinary pairing code** (grants + expiry it picks, capped as below) and shows it as QR + typed code. The new device scans or types it. **No reverse-QR approval.** Check-word confirmation happens on the minting phone. |
| Devices page | Google-style list per computer: what each device can do, last active, "added by", countdown, Sign out. Only on a device holding *Manage devices*. |

### Guardrails on a managing phone (all chosen)

1. **Manage devices** is a new grant that only the computer can give (`wado pair`, `wado allow`,
   TUI). It is never grantable by another device, never self-grantable, never inheritable.
2. **Passkey step-up before every mint** (existing WebAuthn).
3. **A delegate can't exceed its minter**: grants ⊆ the phone's own, never *Manage devices*.
4. **Expiry cap: 7 days** for anything a phone mints. Only the computer grants forever.
5. **Cascade**: signing out a phone signs out every device it minted for.
6. **Hands off computer-paired devices**: a phone may sign out only devices it minted for — never
   the owner, never a computer-paired line.
7. The computer logs and notifies every delegated mint ("Pixel 8 let a device in, until midnight").

## Security implications (stated plainly)

- **Delegation reverses part of `2026-10-30`.** Pairing stays code-only, but a code can now be
  minted on a phone's request, not only by someone at the computer. A stolen *and unlocked and
  passkey-satisfied* managing phone can still let one device in for up to 7 days; the cascade
  ends it when the phone is signed out. This is the residual risk accepted.
- **Social engineering survives every design.** A code proves the device that redeems it is the
  one that was shown it, not that it should get in. Copy on the minting screen must say who the
  access is for, and the expiry cap bounds the damage.
- **Clipboard in full on home**: anyone holding or looking at the unlocked phone sees what was
  copied on every computer, passwords and tokens included. **Accepted by the user `2026-11-04`.**
  The phone's own lock is the only gate.
- **Scanner**: pairing the phone to an attacker's computer is the real threat (phishing desktop,
  clipboard flowing out). Clipboard off by default for scanned-in computers is the mitigation.
- **X server in Game mode**: X11 clients in one Xwayland can read each other's input. Scoped to
  apps whose Game mode turns it on; Details says so.
- **Expiry adds a parse path to `trusted_clients`.** The relay is untouched; the trust file stays
  outside the file manager's reach. A malformed end-time must fail closed (refuse), never open.
- **Web-client keys** live in browser storage. Expiry ("Today") is the default for a delegated
  browser on a borrowed PC.

## Phases

1. **Client only** (Part 1, plus Part 2's top bar, all-computers home, pull handoff and scanner
   safe start): no daemon change. The X-server-on-launch flow orders existing calls.
2. **Daemon, no new trust semantics**: `wado pair` prompt; expiry column + enforcement + TUI.
3. **Delegated pairing** (needs 2): the *Manage devices* grant, a mint-on-request control message
   with passkey step-up, cascade bookkeeping ("minted by" column), Devices page, host notice.
   Update CLAUDE.md's device-trust hazard **when this lands**, not before.
4. **Later**: compositor Game-mode rules; `AppEntry.categories`; start a session with an app.

## Still open (from the storyboard)

Tour card listing permissions? · three tabs or merged Home/Games? · auto-enable Game mode after a
second pad session? · layout picker in setup Q1 or in game only? · busy session + other poster:
side by side or ask? · pill top-left or centre? · ? overlay also lists what's off? · Quit game =
close the app or end the session? · relayed link: block or warn game posters? · Game mode from the
desktop session's own drawer (assumed no)? · dropdown on every Game mode row or Picture only? ·
Steam's inner games as their own posters? · Games tab merged across computers?
