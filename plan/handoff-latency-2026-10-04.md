# wado — handoff (2026-10-04): latency run closed → next: a feature-addition run

## Where things stand
- **Repo:** `/home/bushido/Projects/wado/wado`, branch `main`. Pushing to main deploys the web client to GitHub Pages (about 2 min); confirm with `gh run list -L 1`. Last commit at handoff: `83dc68f`.
- **Read first, in order** (per `CLAUDE.md`): `CLAUDE.md` → `plan/main.md` → `plan/RUNS.md` → `plan/TODO.md` → `plan/memory/shared/environment.md`.
  - `plan/` is gitignored and lives on this machine only.
  - The Decision Log in `WADO_PLAN.md` is authoritative. Today's entries are "Latency compass" and "Auto bitrate".
- **What the latency run found** is in `plan/memory/latency/measurements.md`, section *"RUN CLOSED 2026-10-04"* at the bottom. Don't repeat that work.
- **What changed for users** is in `CHANGELOG.md` → Unreleased (Added / Fixed / Removed).

## The rig (dev environment)
- **Start:** `scripts/rig.sh`.
  - It prints the relay URL and a QR link with a single-use pairing code.
  - Run it with `run_in_background`, otherwise the harness kills cloudflared when the call ends.
  - Needs `dangerouslyDisableSandbox` (GPU).
- **After every release build** (`cargo build --release -p wado -p wado-relay`), restart the daemons with `scripts/rig.sh --daemon`. This ends the user's session, so warn them first.
- **The Cloudflare quick tunnel URL changes** on reboot and on `--stop`. After it changes:
  - update `DEFAULT_RELAY` in `crates/client/src/state.rs`, then commit and push;
  - send the user a new QR. `wado qr --relay URL --id 872990894` prints a terminal QR; the PNG was made by converting the half-block characters with Python.
- **Logs:** `/tmp/wado-rig/daemon-N.log`. Each daemon keeps its last 3 logs (`.1–.3`) across restarts. `/tmp` is wiped on reboot.
- **Perf rows:** `node scripts/perf-report.mjs --since HH:MM[Z] --label … --save`. Rows land in `plan/data/perf-runs.jsonl`; the method is in the script header.
- **e2e:** `node scripts/pool-e2e.mjs`.
  - Build first: `dx build -p wado-client --platform web` plus the release build.
  - About 30 checks, roughly 8 minutes. Wrap it in `timeout 800`.
  - Uses UDP slices 2–3, because the rig owns 0–1.
  - "Join session takes it back" fails about 1 run in 3 and passes on rerun. It hasn't been investigated.

## Hazards learned this run (beyond CLAUDE.md)
- **Bridge JS:** `bridge.rs` concatenates all `js/*.js` files into one eval. A runtime throw at load time in any file silently drops every file after it, and `lifecycle.js` (which defines `W.start`) is last. Never run loops synchronously at load.
- **Data channels share one SCTP association**, and therefore one congestion controller. High message rates stall everything on that connection, input included.
- **Settings sync:** the daemon stores each device's settings copy (`config_set_prefs`). The newer copy now wins via an `_at` timestamp. Before that fix, a reload reverted recent changes.
- **Stray processes:** kill wado processes by exact pid, matched on environment or cmdline. `pgrep -f` with a pattern also matches your own shell.

## Open threads (the user may come back to these)
1. **Stream sound A/B was never run on the phone.** The switch kept reverting until fix `83dc68f`. Test: Stream sound off → reload → join. If the video buffer stays around 15 ms, the audio-sync hold is confirmed, and the next step is a design choice for the user (e.g. sound only while media plays).
2. **Trial options with no measured gain:** Redundant input, Low-latency audio, Keyframes "On request". The user may ask to remove them; the list of what to remove is in `plan/TODO.md`, section "⚖ Trial".
3. **Viewer audio sink** (latest change): created on connect and named "wado · <device>", recreated for a new device, removed 30 s after the device leaves unless a session is still playing into it. The e2e verifies naming and the idle daemon; real-device behaviour hasn't been confirmed by the user yet.

## User preferences
- Commit and push after every change; short commit messages, with the detail in `CHANGELOG.md`.
- New optimisations and behaviour changes go behind switches on **Display & stream** (memory: `feedback_optimisations_as_options.md`).
- Always run the daemon from a release build.
- Measure before claiming, and say so when a fix rests on reasoning rather than measurement.
- Present facts and paths, then ask through multiple-choice questions when direction is ambiguous.

## Suggested skills
- `grill-me`: before designing the new feature, to settle its decision tree with the user (they like multiple-choice questions).
- `diagnose`: for any bug or regression report; reproduce and instrument before fixing.
- `tdd`: for new logic modules. The repo's style is small, unit-tested modules plus an e2e check per feature.
- `viz-reports`: if the user asks for a write-up of findings.
- `handoff`: at the end of the next run.
