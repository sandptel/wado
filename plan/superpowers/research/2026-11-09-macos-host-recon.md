# macOS host — recon (2026-11-09)

Input for the macOS backend design. Two read-only surveys: wado's Linux seams, and how Sill
(`~/mine/sill`, Swift, github.com/Saffsanity/sill) does the Mac host side. Line numbers are as of
this date — leads, not facts.

## wado: what blocks macOS today

- `crates/server` does not build on macOS: hard dep on `wado-compositor` (smithay `backend_gbm`,
  calloop `signals`), Linux-only libc (`prctl(PR_SET_PDEATHSIG)` in `platform/linux/inhibit.rs:60`,
  `viewer_sink.rs:92`, `files/mod.rs:77`; `openat2`/`O_PATH`/`renameat2` in `files/scope.rs`,
  `files/list.rs:25`, `files/quick.rs:119`), `platform/mod.rs` declares `linux` un-gated and
  `os_default()` always returns `LinuxProvider`.
- Video seam exists: `wado_platform::video::VideoBackend { model: OwnSession|Mirror|VirtualDisplay,
  thread: Any|Main, encoders(), build(frame_tx) }` (`platform/src/video.rs:18,50-67`). Smithay is
  the only impl (`compositor/src/backend.rs`). Built at `server/src/main.rs:49-51`, runner on main.
- **The real refactor:** `Handles` is concrete `CompositorHandles` (`compositor/src/lib.rs:86-111`):
  calloop `CommandSender` / `InputSender`, plus watches (timings, text_input, shedding, windows,
  menu, clipboard, app_bus). Server names these in ~10 files (`relay_client.rs`, `viewers/`,
  `website/`, `apps/running.rs`, `a11y.rs`, `clip/sync.rs`). `CompositorCommand`
  (`compositor/src/control.rs:29-104`), `InputItem`, `Clip`, `MenuSpot`, `FrameMsg` need to move to
  a neutral crate; senders become backend-neutral.
- Frame path is already neutral: `FrameMsg { data: Annex-B AU, duration, queued_at }`
  (`sink/channel.rs:30`), drop-on-full `try_send`, one H.264 `TrackLocalStaticSample`,
  `write_sample` pump. Keyframe = `CompositorCommand::ForceKeyframe`. Bitrate = `Reconfigure`
  rebuilds the encoder (no live rate control — matches VideoToolbox, below).
- Encoder trait `VideoEncoder` (`compositor/src/encode/encoder.rs:19-28`) takes
  `capture::Frame::{Rgba, Dma(&smithay Dmabuf)}` — compositor-internal; a Mac backend owns its own.
- Platform providers: traits for Touch/Key/Text/Gamepad injectors, Clipboard, PowerInhibit,
  PtySpawner, Launcher, WindowManager, etc. Only `LinuxProvider` exists; no macOS stub. Key codes
  are evdev everywhere ⇒ macOS maps evdev → kVK.
- Audio capture (`server/src/audio/capture.rs:24`, `pw-record`) is **not** behind a trait; Opus
  encode is portable.
- Protocol Linux-isms a Mac host fakes: `WindowInfo.app_id` (xdg), `AppEntry.exec` (XDG Exec=),
  `SessionConfig.isolate_apps/x_server/preset`, `HitWindow`/`MenuSpot` (AT-SPI). `WindowInfo.id`
  u64 fits a CGWindowID; `EncoderReport` strings are free-form.
- Prior design: `plan/superpowers/specs/2026-10-30-platform-layer-design.md` already expects a
  macOS `VideoBackend` with `thread: Main`.

## Sill: the Mac host, what it learned

Capture (ScreenCaptureKit, `Sources/SillHost/WindowCapture.swift`):
- 420f NV12, `minimumFrameInterval = 1/fps`, `queueDepth = 5`, `showsCursor = false` (cursor sent
  separately), sRGB. Window: `SCContentFilter(desktopIndependentWindow:)`; desktop:
  `display:excludingApplications:[self]`.
- **Never `updateConfiguration` a live SCStream** (wedged on macOS 27) — stop + new stream; drain
  the handler queue after stop.
- Keep only `SCFrameStatus.complete`. A still/covered window emits nothing ⇒ keep last frame and
  re-encode as IDR when idle ≥50 ms / on keyframe request.
- Capture scale = `min(want, filter.pointPixelScale)`, even sizes; asking 2× of a 1× window gives a
  top-left quarter.
- CLI: `CGMainDisplayID()` on main thread first (`CGS_REQUIRE_INIT`). `SCShareableContent` can hang
  forever (no Screen Recording + virtual display) ⇒ timeout every call.

Virtual display (private `CGVirtualDisplay`, `VirtualDisplay.swift`, `VirtualStage.swift`):
- Resolved via ObjC runtime with type-encoding checks (BOOL is "B" arm64 / "c" Intel). Stable
  vendor/product/serial. One mode + HiDPI; force 2× via `CGConfigureDisplayWithDisplayMode`;
  min side 800 pt. Needs `NSApplication.run()` (not `dispatchMain`).
- Move window via AX position→size→position; restore window **before** destroying display; atexit
  restore. Fallback to plain capture after 2 failures. Not App-Store-able.

Encode (VideoToolbox, `HEVCEncoder.swift`; H.264 works the same per `CaptureProbe --h264`):
- `RealTime`, `AllowFrameReordering=false`, `MaxKeyFrameInterval=fps*4`, `AverageBitRate`,
  `DataRateLimits=[bitrate/8,1]`, `PrioritizeEncodingSpeedOverQuality`. **Not**
  `EnableLowLatencyRateControl` (dropped frames at 8 Mbps).
- Live bitrate change ignored by HW ⇒ new session. Force IDR with
  `kVTEncodeFrameOptionKey_ForceKeyFrame` (+ retry ×3 if dropped).
- One frame inside VT at a time, one-slot mailbox (newest wins). Watchdog 1.5 s ⇒ software
  encoder. **Never invalidate a session with a frame inside** (wedged HW encoder system-wide until
  reboot) — `CompleteFrames` first, off-thread.
- HW "slow state": after ~1 s sparse frames, ~29 ms/frame (~34 fps cap) ⇒ replace session.
- Measured: capture→encoded 17.6 ms median; encode 9 ms at 3024×1964; ~3% CPU; M2 Pro ~650 MP/s.

Input (`InputInjector.swift`):
- `CGEventSource(.hidSystemState)`, post `.cghidEventTap`. Drag = `*MouseDragged`. Click count via
  `mouseEventClickState`. Scroll: `scrollWheelEvent2` pixel units, continuous, ScrollPhase xor
  MomentumPhase; carry sub-point remainders.
- Text: keycode 0 + `keyboardSetUnicodeString`, **flags = [] explicitly**. Keys: explicit flags,
  clear modifiers on key-up; host does auto-repeat.
- Gestures: read symbolic hotkeys (private `CGSGetSymbolicHotKeyValue`) and post the chord.
- No Accessibility ⇒ events dropped silently; grant belongs to the parent app (Terminal).
- Activate apps via AX `kAXFrontmost`, not `NSRunningApplication.activate`.

Audio: separate audio-only SCStream (48 kHz stereo, `excludesCurrentProcessAudio`, per-app filter),
no extra permission. Sill uses AAC-ELD; wado would feed its existing Opus encoder.

Other: clipboard polls `NSPasteboard.changeCount` 4 Hz only while subscribed; launch via
`NSWorkspace.openApplication(activates:false)`; menus via AX with per-element 1 s timeout.

Signing: TCC grants keyed on bundle id + signing identity — ad-hoc rebuilds lose Screen Recording
and Accessibility every build; a CLI's grants belong to its parent (Terminal).
