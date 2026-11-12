# CLAUDE.md — wado

Rebuilt `2026-11-09` from citations of the old, never-committed copy; correct it when you know better.
Overview: `README.md`. App build: `docs/app.md`.

## The `plan` branch — where every agent file lives

All agent material — this file, plans, specs, memory, design docs — lives in `plan/`, which `main`
ignores. It is tracked on the orphan branch **`plan`** (nothing else on it, never merged either way).

**After any change under `plan/`, commit it before the turn ends:**

```sh
plan/commit.sh "what changed"     # commits plan/ to refs/heads/plan via a side index, then pushes
```

`git status` never shows `plan/` edits. Skipping this is how the old `CLAUDE.md`, `WADO_PLAN.md`,
`TODO.md` and `CHALLENGES.md` were lost. Never put plan edits in a commit on `main`.

Pull another machine's plan changes (commit yours first — this overwrites `plan/`):
`git fetch origin plan:plan && git restore -s plan -W plan`.

Write new specs and plans to `plan/superpowers/{specs,plans,research}/`.

| | |
|---|---|
| `plan/main.md` | Compass for runs. "Start a run" → `plan/RUNS.md` |
| `plan/TODO.md` | The live list — open, blocked, awaiting the user |
| `plan/HANDOVER.md` | State at v0.0.4 + the Flutter port; pick work up one item at a time |
| `plan/issues.md` | Observed, attributed, unfixed. Leaves when the fix is seen working |
| `plan/memory/` | What runs learned, by lane. `shared/environment.md` first, every run |
| `plan/design/` | `DESIGN.md` (visual system), `PRODUCT.md`, `.impeccable/` |
| `plan/superpowers/` | Specs, plans, research, prototypes |
| `CHANGELOG.md` (main) | What shipped |

`WADO_PLAN.md` (architecture + Decision Log) is lost; Decision Log dates cited elsewhere
(e.g. `2026-10-29`, `2026-10-31`) are the only trace. Recreate entries in `plan/` as decisions recur.

## Invariants

1. **Input never rides behind anything that can head-of-line block it** — not video, files,
   presence or the async runtime. Own channels / peer connections. The relay fallback is the one
   logged exception (Decision Log `2026-10-29`).
2. **Video goes over UDP WebRTC.** The relay carries signalling, never video.
3. **The host-memory (CPU) path keeps working forever.** Fallback ladder `vaapi-dmabuf` →
   `vaapi-cpu` → `x264-cpu`; a runtime encode failure downgrades one tier and keeps streaming.
4. **H.264 by default.** H.265/AV1 only if every participant has hardware decode.
5. **Software encoding is always shown** to the user; that status is never dropped for lack of room.
6. **Opening is the probe.** A capability (encoder, `/dev/uinput`, tool) is offered only when
   opening it works here — never by name or config alone. What is missing says why.
7. **Encoder tuned for latency, fixed at session start:** CBR, no B-frames, no lookahead,
   keyframes on demand rather than periodic.
8. **A new size is a fresh `Output`** — never resize one in place.

## Hazards

- **Direct-mode HTTP control listens on localhost on purpose** — exposing it is remote code
  execution. No auth yet; localhost / LAN only.
- **Browser and daemon clocks are not synchronised.** Never add their times together; read
  breakdowns as proportions.
- **Debug builds are not data.** Measure only `./target/release/wado`.
- A broken session must not take the daemon down (crash isolation: panic guards on event sources).
- Device trust: update this entry when device-key work lands (see `plan/HANDOVER.md`).
- More traps, with dates: `plan/memory/shared/environment.md`, `build-deploy.md`, `verification.md`.

## Pins

- Smithay: a git revision, pinned in `Cargo.lock` only (deliberately); its hash is in `flake.nix`.
- `webrtc-rs` 0.17 — known wart: `write_rtp` blocks on its 256-deep bounded channel.
- `flutter_rust_bridge` 2.13.0 exactly — crate, Dart package and codegen must match. Never `pin!`
  in `crates/app-bridge`.

## How to work

- **One job per file** (atomic module separation). Server code holds no Smithay type.
- **Measure before fixing**, with a number before and after. Label estimates as estimates.
- **Ambiguity → ask the user**; don't guess at intent.
- `cargo fmt --all --check` green before committing to `main`.
- **Commit messages: short, one line** (`scope: what changed`), no body. **Never mention Claude/AI**
  — no `Co-Authored-By`, no session links, no "generated with". Same for PRs and the `plan` branch.

## This machine

macOS dev box (set up `2026-11-09`). Daemon/compositor are Linux-only (Smithay, VA-API, uinput) and
the Nix flake targets Linux. Not installed here: nix, flutter, adb, dx, `flutter_rust_bridge_codegen`.
Remote is HTTPS via `gh` credentials (no SSH key).
