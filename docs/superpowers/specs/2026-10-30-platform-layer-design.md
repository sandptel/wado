# Platform layer — host capabilities behind traits (approved blend, 2026-10-30)

Chosen by the user after comparing three designs (minimal ops-as-data, per-capability traits,
services-and-ports). This is the **blend**. It exists so macOS/Windows backends and the Slint
desktop app can implement or consume host capabilities selectively, and so every capability is
faked in tests.

## Principles

1. **Hot path: one small sync device interface, never awaits.** All three designs agreed on this
   — it is today's `pad::state::Sink`, generalised:
   `TouchInjector { resize, frame, lift_all }`, `KeyInjector { tap, hold, release_all }`,
   `GamepadInjector { report }`, opened through a factory `Open<D>` so devices are owned by one
   input lane. Evdev codes are the lingua franca (platforms map to kVK/VK).
2. **Control plane: one typed trait per capability family** (design B): `TextInjector`,
   `AudioMixer`, `ViewerSink`, `MediaPlayers`, `Brightness`, `WifiRadio`, `Bluetooth`,
   `PowerInhibit`, `SystemPower`, `Clipboard`, `ClipHistory`, `WindowManager`, `Launcher`,
   `Screenshot`, `SysInfoSource`, `Trash`, `Thumbnailer`, `PtySpawner`. Async methods return a
   hand-written boxed future (`type Fut<'a,T> = Pin<Box<dyn Future<Output=T>+Send+'a>>`) — no
   `async-trait` dependency; owned arguments.
3. **Registry with honest absence:** `Platform { audio: Cap<dyn AudioMixer>, … }` where
   `Cap<T> = On { imp, by: ProviderId } | Off(Absent { why, tried })`. Built by
   `PlatformBuilder::os_default()/empty().with(provider).prefer(cap, provider).without(cap).probe()`;
   providers register probes, **opening is the probe** (invariant 6), results cached,
   `reprobe(cap)` for hot-plug. `Platform::caps()` → a serialisable `CapSet` the clients use to
   hide what's missing (actions keep their `needs`, expressed as capability requirements).
4. **State is pushed, not polled** (from design C): each stateful capability publishes its latest
   state on a `tokio::sync::watch` channel (`fn state(&self) -> watch::Receiver<S>`), so the
   Computer page, home widgets and the Slint app subscribe; `Platform::host_state()` assembles the
   wire `HostState` from the watches with no I/O. Edges go on `broadcast` where needed.
5. **Isolation only where it pays** (from design C): an optional `service` wrapper runs a backend
   on its own thread behind a `Port` (command/input lanes in, watch/events out, panic → restart).
   Use it for **video** and the **slow D-Bus services** (NetworkManager, BlueZ, MPRIS) so a hung
   `nmcli`/bus call can't freeze others. Everything else is a plain trait object.
6. **Video is a runner that declares its thread** (`VideoBackend { model: OwnSession|Mirror|VirtualDisplay,
   thread: Any|Main, encoders() (probed+cached), build(frame_tx) -> (Box<dyn Runner>, handles) }`),
   so macOS (NSApp on main) and a Slint app can share one main loop (a "run inside an existing
   loop" variant is part of the design). The compositor/server boundary is unchanged: the
   Smithay backend implements `VideoBackend` inside `wado-compositor`; the server never names
   Smithay types.
7. **Policy stays above the traits.** Grants, Wi-Fi probation, pacing, stale-frame handling,
   viewer-sink lifecycle are OS-neutral server logic, written once.
8. **Fakes for everything** behind a `fake` feature, plus a contract test every real provider must
   pass.

## Crates

- `wado-platform` (new, Smithay-free, no network types; depends on `wado-protocol` wire types and
  `tokio` with `sync` only): the traits, `Cap`, `Absent`, `Platform`, builder, `CapSet`, the
  `service` wrapper, fakes. This is the MAC0 "wado-backend" crate, renamed/merged (record in the
  Decision Log).
- Providers: Linux first, by moving today's code behind the traits (`pad/uinput`, `host/*`,
  `actions/*`, `clip/*`, `files` seams, `pty`) — in `crates/server/src/platform/linux/` initially
  (a separate `wado-host-linux` crate later if it pays). macOS/Windows providers come with their
  milestones.

## Migration order (each step behaviour-preserving, with a perf pass on the code it moves)

1. Create `wado-platform` with traits, registry, fakes, contract test.
2. Input: pad devices → `TouchInjector`/`KeyInjector`/`TextInjector`; gamepad.
3. Host controls: audio/viewer sink, media, brightness, Wi-Fi, Bluetooth, awake, sysinfo, system power;
   `host_state()` from watches (removes per-request wpctl/zbus fan-out).
4. Actions → `WindowManager`/`Launcher`/`Screenshot`/`SystemPower`, `needs` as capability reqs.
5. Clipboard + history; files seams (trash, thumbnails); PTY.
6. Video: `VideoBackend` for the Smithay compositor, behind the service wrapper.
Each step: tests via fakes; existing e2e scripts stay green; measured before/after where it touches
a hot or polling path.
