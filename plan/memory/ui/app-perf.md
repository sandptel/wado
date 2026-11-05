
## Android app frame cost (2026-11-03)
- The emulator renders on SwiftShader (CPU): its raster ms are 10–50x a phone's and say nothing
  absolute. Trust build ms, and *how many* frames are produced while idle.
- Any forever loop (repeat() with no count) keeps the whole scene compositing every vsync:
  measured 90–120 slow frames per 3 s idle on home/computer page. Breathe now rests after 3
  cycles; `cycles: 0` only for in-progress states. Check idle frame count after adding motion.
- Release builds log `slow frame` (>100 ms) and `main thread held` (>400 ms) to Diagnostics'
  log — ask the user for that log with a stutter report.
- **Dart runs on Android's main thread** (Flutter ≥3.29 merged it; 3.47 *refuses* the
  `DisableMergedPlatformUIThread` opt-out at startup — the app dies). So every native plugin call
  (flutter_webrtc) holds the screen while it runs. Measured per session start on the emulator:
  `renderer.initialize()` 140–380 ms (now paid under the splash, `Session.warm`), the video
  track's `srcObject` 280 ms (flutter_webrtc releases + re-inits its EglRenderer there; only a
  plugin patch removes it — not done), a 900 KB `apps_list` jsonDecode 41 ms (now off-isolate
  above 64 KB, order kept). Find stalls with the release `main thread held` log, then
  `adb root` + `debuggerd -b <pid>` samples, symbolised with `nm -n` on the profile libapp.so.
- Profile builds log `main thread held` over 280 ms and each slow frame's `waited` (vsync
  overhead); release keeps 100 ms / 400 ms.
- Touchpad size goes to the daemon in **logical** px (the web's CSS px): ×dpr made the virtual
  pad ~3× larger in mm, past libinput's gesture/thumb thresholds (fixed 2026-11-03).
- Haptics go through the vibrator's predefined effects (`feel()` → MainActivity `buzz`), not
  `HapticFeedback`/`performHapticFeedback`, which the system's touch-feedback setting mutes.
- **Never `ListenableBuilder(listenable: Settings.I)` around anything big** (2026-11-04): Settings
  notifies on every key, so a volume slider rebuilt all of home and the widgets per tick. Listen to
  `Settings.I.keys([...])` (kept for the widget's life, never made in build), and give each home
  section its own sources. Colour tokens (`s1`, `dim`, `line`…) are `late final` per theme — an
  OKLab mix per read was thousands of pow/cbrt per frame.

## Main thread = video frames (2026-11-05, code review — unmeasured)
- With Dart on Android's main thread, each video frame's re-composite is scheduled through that
  thread too (SurfaceProducer `markTextureFrameAvailable` → animator). Any main-thread hold is a
  late picture, not only a slow page. Plugin calls (flutter_webrtc, MediaSession, notify) are the
  part neither Dart isolates nor Rust can take — only F9b (native session) removes them.
- Found and fixed: now-playing re-sent every poll (position in its dedupe key) with art → binder
  calls with a bitmap every 1.2 s; `noteHost` with the full state every poll (now only on `info`
  change); `Host.I.state` replaced every poll (now deep-compared); `getStats()` whole connection
  1 Hz (now the video receiver's, falls back if no candidate-pair).
- **Covered pages still build.** Offstage + TickerMode(false) does not stop ListenableBuilders: home
  and the computer page rebuilt under the session on every poll/setting. Page-level builders use
  `Live` (`ui/widgets/live.dart`), which ignores notifications while TickerMode is off.
- **Impeller has no raster cache**: RepaintBoundary saves repaint, not raster; an Opacity < 1 over a
  page is a full offscreen pass per frame. The shell comment claiming otherwise was corrected.
- No refresh-rate request existed; *Full refresh rate* (off by default) sets
  `preferredDisplayModeId` to the fastest mode at the current size.
