# wado-desktop (experimental)

The wado desktop app, in Slint. One app, two modes:

- **This computer** — the host console for a computer running `wado daemon`: overview (Remote
  ID, relay, daemons, live stats, activity), devices and grants, pairing (QR, typed code,
  countdown, the four check words), sessions, the daemon log, and every section of config.kdl.
- **Other computers** — the client: the computers this app knows, whether each one's relay sees
  it, and a Computer page with its doors.

Not a default member of the workspace: build it by name.

```sh
cargo build -p wado-desktop
cargo run -p wado-desktop                      # host mode if a daemon runs here, else client
cargo run -p wado-desktop -- --client          # or --host; --scheme catppuccin-latte
cargo run -p wado-desktop -- --fixture         # a made-up computer, nothing real touched
cargo run -p wado-desktop -- --screenshot DIR --fixture   # every screen, headless, as PNG
```

`--screenshot` renders with Slint's software renderer (no display, no GPU). It draws gradients
with square corners, which the window's femtovg renderer does not; compare with the window.

## Shape

| Where | Job |
|---|---|
| `ui/` | The Slint UI, compiled at build time. `theme.slint` holds every token; `state.slint` the three globals Rust fills (`App`, `Host`, `Client`); one file per page and per widget family. |
| `src/caps/` | **The only code that calls the daemon's library or touches the host.** Shaped after the platform layer (`docs/superpowers/specs/2026-10-30-platform-layer-design.md`): `Cap::On { imp, by } \| Off(Absent { why, tried })`, and state pushed on `tokio::sync::watch` channels. |
| `src/bridge.rs` | A `watch` receiver into the event loop with `upgrade_in_event_loop` — the UI never polls. |
| `src/host/`, `src/client/` | Each mode's UI-thread state, models and actions, one file per page. |
| `src/theme.rs` | The web client's `schemes.json`, mixed in OKLab exactly as `base.css` mixes it. |
| `src/shot.rs` | `--screenshot`. |

Reused from the daemon, not re-implemented: the trust list and grants (`wado::gate`), pairing
codes and the connect link (`wado::cli::qr::pair_link`), the check words (`gate/confirm.rs`),
the pool's status (`wado::cli::status`), sessions and links, and config.kdl through
`wado_config` (the schema itself is serialised for the field list; the starting config's
comments give the hints and choices; writes go through `ui_file`, which validates against the
whole config and undoes a bad write). `wado::cli::status::https_get` was made public for the
client's online probe.

## Stubbed or partial

- **Native desktop streaming is not in this build.** Desktop, Trackpad, Files and Shell open the
  web client in the browser at that computer, and the page says so. The plan is in
  `src/caps/video.rs` (pairing the app as a device, webrtc-rs 0.17 receive, VA-API decode, frames
  into a `slint::Image`, then DMA-BUF import).
- The encoder tier is read from the daemon log (`pipeline=…`, `session ready — encoder …`) and
  labelled "last seen in the daemon log": nothing outside a session publishes it yet.
- A computer's sessions are "known once connected" on its page: there is no live link to it yet.
- `session.env`, `server.turn` and `window-rule` are shown, edited in config.kdl.

## Licence

This crate is **AGPL-3.0-only**. Slint 1.18 is offered under
`GPL-3.0-only OR LicenseRef-Slint-Royalty-free-2.0 OR LicenseRef-Slint-Software-3.0`; wado takes
it under **GPL-3.0-only**. GPL-3.0 §13 and AGPL-3.0 §13 each expressly permit combining a work
under one with a work under the other into a single combined work: each part keeps its own
licence, and the AGPL's network-use clause applies to the combination. So the app as a whole is
distributable under the AGPL with every source available, which the royalty-free licence (with
its attribution and platform conditions) would not add anything to. The bundled fonts —
Bricolage Grotesque, Figtree, JetBrains Mono — are SIL OFL 1.1 (`fonts/OFL-*.txt`), which allows
embedding them in software under any licence.
