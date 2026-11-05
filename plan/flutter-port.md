# Flutter port — one long session (started 2026-10-31)

User's ask (verbatim): "take the webview ui that runs on browser currently and has everything perfectly
done ... run a long single session and port it exactly to flutter ... I will review after its done".
No phases, no parallel agents, no mid-way reviews. User reviews at the end.

## Architecture (decided for the port)
- Rust `client-core` = secure pipe: relay handshake, e2e, seal, outbox, reconnect. NEW: link rooms
  seat | control | view{instance}; every opened plaintext message also goes out RAW (JSON string)
  so Dart ports the JS feature modules 1:1 (W.relayOn(type) / W.relaySendMsg model).
- `crates/app-bridge` = flutter_rust_bridge 2.13.0 façade.
- `app/` Flutter project. Dart ports of crates/client/src/js/*.js and ui/*.rs, same names (snake_case).
- Installs beside the old app as io.github.sandptel.wado.next ("wado next").
- Video/data channels: flutter_webrtc.

## Progress (tick as done; newest notes at bottom)
- [x] S0 flake .#app shell (flutter 3.47, frb codegen 2.13) — `nix develop .#app`
- [x] S0 core: rooms + raw messages + open/close/send API (commit 'core: seat, control and view rooms…')
- [x] S0 app-bridge + codegen (commit 6c374bf)
- [x] S0 Flutter skeleton builds on linux (android build not yet tried)
- [x] S1 Dart link layer: RelayLink (seat). side_link (control rooms for widgets) not yet
- [x] S2 theme + CSS tokens
- [x] S2 home (cards, running, on this phone)
- [x] S2 pairing (link/typed; no QR scanner yet)
- [x] S2 computer page (hero, actions, now playing, quick, settings list)
- [x] S2 settings pages: display_session, sound_devices, connection_trust (input_feel, tiles, trackpad/keyboard/gamepad prefs NOT yet)
- [x] S2 passkey (Kotlin Authenticator via MethodChannel 'wado/passkey'; webauthn bytes in app-bridge)
- [x] S2 session: video, input (pointer mode), dock, control centre (no console/cockpit surround yet)
- [x] S2 touchpad (pad link relayed+direct, frames bin, why-card, rail) — settings page not yet
- [x] S2 keyboard (pad keys via phone IME on the pad screen)
- [x] S2 gamepad (Standard layout, sessionless) — variants/editor not yet
- [x] S2 files + viewer (image/text; media/pdf as download)
- [x] S2 shells (xterm)
- [ ] S2 clipboard
- [ ] S2 push/notify, claude, widgets
- [ ] S3 APK to ~/Downloads, docs, HANDOVER

## Notes

- Codegen: `nix develop .#app -c flutter_rust_bridge_codegen generate` from repo root (flutter_rust_bridge.yaml).
  Gotcha: never use `pin!` in app-bridge (expands to `super let`, frb's syn can't parse). Bootstrap needed an
  existing frb_generated.rs.
- Bridge events are JSON strings: computers / room_state{id,state,words,tone,caps,instance,in_ms} /
  room_msg{id,json} / room_relay{id,json} / pair{op,step,...} / answer{op,outcome,why}.
- Dart files so far: lib/core/core_host.dart (CoreHost.I: open/send/close rooms, pair, forget, computers),
  lib/wado/{log,relay_link,settings}.dart (RelayLink.I.dial/on/send/takeover; Settings.I.get/set),
  lib/ui/theme/{tokens,look}.dart (Tokens w/ OKLab mix; Look.I; Tok.of(context)),
  lib/ui/widgets/{basics,icon_paths}.dart (WIcon, Disc, Press, Sect, ListBox, NavRow, WCard, Btn, IconBtn,
  WhenBadge, InfoText, Sw, SwitchRow, Seg, Hint, Why, rem(), body(), display(), mono(), slot()).
- main.dart → app.dart → ui/shell.dart (TO WRITE). main must `await Settings.I.load(); await Look.I.load();`.
- Linux CMake installs target/<debug|release>/libwado_app_bridge.so into bundle lib (app/linux/CMakeLists.txt).
- Fonts bundled (OFL): Bricolage Grotesque, Figtree, JetBrains Mono. OS logos: app/assets/os/*.svg (os-logos.mjs).
- Web structure learned: View = Home | Computer(key) | Sub(key?, Page). Selecting a computer dials its seat
  (relay_link.js); others get onlineWatch HTTP probes `<relay>/online/<id>` → {daemons}. Side links (control)
  for widgets. relay.js: __up → onLinkUp (session_rejoin / session_start{config}); session_started → negotiate
  WebRTC (W._relayNegotiate); session_alive → choice (rejoin/drop).
- Testing without touching ~/.config/wado: run a throwaway relay + release daemon with temp XDG dirs (as
  scripts/core-e2e.mjs) and pair the app to THAT. Never pair to the user's real daemon myself.

- 2026-10-31 milestone: full path verified on Linux headless (scratchpad rig.sh): pair → seat link → session_start
  → hardware encoder → WebRTC via flutter_webrtc → video on screen. Rig = throwaway relay :4993 + release daemon
  with env from ~/.cache/wado/daemon.env, XDG_RUNTIME_DIR=/run/user/1000/wrig (short: SUN_LEN), weston headless
  kiosk --renderer=gl (sandbox off), screenshots via weston-screenshooter. Build: `flutter build linux --debug
  --dart-define=WADO_DEV_BOX=… --dart-define=WADO_E2E_LINK=… --dart-define=WADO_E2E_SESSION=true`; run with
  LD_LIBRARY_PATH=$WADO_APP_LIBS. libwebrtc needs libpulse (ADM aborts otherwise).
- Known gaps to fix: session res came out 1280x720 (check Session.config res default); dock 'keyboard' icon name;
  input not verified yet; Android build not tried.

- 2026-10-31 later: Android verified on emulator (PIN 1234; adb reverse for the rig relay). Fixed: one core per
  process (Android reruns main), lock only on WouldBlock (flock errors on Android storage), settings reads never
  null-crash. Pocket Console, control centre tiles, passkey (Kotlin channel), clipboard, files, shells, pad all in.
  Remaining gaps listed in HANDOVER.md §0.

## User reports on wado-next-0.1.0 (2026-10-31, after the first phone test)
User asked: note the reports, fix them, and port everything still unported (HANDOVER §0 list).
- R1 **QR pairing does not work.** Cause found on the emulator: the app never asked for CAMERA at
  runtime → zxing's CameraX saw no camera, black preview, no prompt. Fixed: `wado/os` channel
  `camera` asks first; decoder now tryHarder + tryInverted (light-theme terminal QRs are inverted;
  verified with zxing-cpp host-side) + 80% crop + single pop. Phone camera app → browser, not the
  app: added wado:// + https client-link intent filters.
- R2 **Animations and design feel like a cheap copy of the web UI; many features/UI quirks don't
  show up at all.** Method: screenshot web pages vs Flutter pages side by side on the rig, list
  every difference (motion, layout, missing quirks), port them one screen at a time.

## Fidelity pass (R2) — checklist, in order (tick as done; commit after each)
Method: web screenshots via scratchpad webshots.mjs (?fixture, 412×915 @2.625) vs emulator; motion
ported from the CSS keyframes/JS of each component (ui/motion/motion.dart: Enter, Flip, Glide,
Breathe, Bump, ArtSwap; ViewClock). Emulator: animator scale must be 1 (it was 0 → Flutter ran
animations at 5%); WADO_E2E_SLOW=N dilates time for filming. scratchpad tap.sh taps by a11y label.
- [x] R1 QR: camera permission; decoder hardening; wado:// + https links; share target; icon/splash
- [x] Nav motion: hero flip card→page and back, home recede, section rise, sub push/drift
- [x] Press: immediate sink (Listener), per-part press (card .98 + disc .92, cgo .9 + glyph nudge)
- [x] Haptics (phone constants) + active (keep screen on, immersive) for session/pad/gamepad
- [x] Computer page: action tiles (VI doors, badges), Sessions (multi: join/takeover/end, link test,
      start new, instance-sticky seat), Claude section+picker, full clipboard (+auto sync), toasts,
      prompt sheets, clip dock, now-playing swaps, quick bump
- [x] Settings pages: head + Apply bar, Display & session (+host limits/swipes/shortcuts/keys),
      Sound & devices (+listen), Connection & trust (+Wi-Fi, owner, fetch, forget), Appearance
      (text/interface size applied app-wide), Input feel, Diagnostics (+server log page)
- [x] Quick tiles page + control-centre tile pool (cc/tiles/pool.rs). Defaults swap Take mouse for
      Computer awake (no pointer lock on Android); Screenshot/Record/Take mouse not in the pool yet
- [x] Dock & switcher (arrangement.rs, switcher.rs) on Appearance; console_open + dock_pin honoured
- [x] Home: widgets (light links = core control rooms), edit/reorder/picker/tray, Running motion
- [x] Session: Pocket Console (console_view + parts + deck + stage_state), cockpit morph (one
      controller, fly() between rects, plates fade), dock/rail/workspaces/window sheet, link badge,
      people chip, drawer, control centre (hero/meters/tiles/sound/pushed pages), stats reader
      (whole session) + health verdict + autorate (tests), readouts (pill, health, touch marks),
      presence ghosts, viewer_visible on hide/show. Verified on the emulator against the rig.
- [x] Session leftovers: latency breakdown readout (latency.js), reconnecting overlay, lane hint,
      rejoin card polish, input preview (input_preview/), cockpit orientation choreography
- Decisions logged for review: portrait console with a landscape session puts the faces in a row
  over the deck (the web overlaps them with Rotate — same bug there, not fixed in the web); the
  drawer sits over the dock (the web's dock covers its bottom row).
- Found + fixed: home cards were `const Cards()` so they never rebuilt on link changes (Start
  stayed disabled on "Connecting…"); pages under a session kept ticking and exposed semantics.
- [x] Passkey step-up sheet (ask(need, then): Claude allow/open/new, touchpad); lock glyph tug +
      swing, approve-on-computer waiting card, shake notes. Card on pages unchanged.
- [x] Files viewer by kind (kinds.dart + test, markdown, JSON, CSV table, archive entries via the
      daemon, PDF/video/audio handed to the phone's app via FileProvider `view`); uploads stream
      from disk (uploadStream). NOT exercised on the rig (the test device has no files grant).
- [x] Touchpad: landscape rail (latch/lock mods, keys, Type, settings sheet), surface rings/halo/
      connector/count/glow, coach, why-card over the surface, portrait typing layout, passkey first.
      Seen on the emulator only via a temporary local gate bypass (not committed).
- [x] Gamepad restyled to the web's default look + bar (close, computer, P1).
- [x] Native: notifications (session apps while behind; pushed notes), media session for the
      computer's now-playing, volume keys, Stay connected service, UnifiedPush (connector with tink
      excluded — flutter_secure_storage brings tink-android), Notifications & background page.
- [x] Gamepad layouts as data (gpad_layouts.dart + test), picker, console face; pad quick actions
      (pad_actions.dart, rail/bar, confirm disc, chooser); Screenshot tile; reconnecting overlay.
- [x] 0.3.0: drawer (asking/re-ask/cache/SVG), deck put-away, stutter cuts, session fits the
      console (pictureRoom), latency chips, lane hints, Record tile, pad gesture hints + phone
      gestures + settings, files overview + selection, gamepad editor (Place only), End refusal fix.
- [ ] Left: Take mouse (no pointer capture on Android), gamepad editor Look/Behave + Save as.
- [ ] iOS/macOS (later)
- Emulator crashes (qemu SIGSEGV) when a page is pushed over the computer page: Impeller on the
  emulator's emulated GLES. Skia does not crash. Test builds: `--android-project-arg impeller=false`
  (manifest placeholder; release keeps Impeller). Also fixed: layer wrappers changed shape on
  completion → the page underneath was rebuilt; now one shape + TickerMode off when hidden.
