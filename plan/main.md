# plan/main.md — the compass

Entry point for every run. Start here, then follow the pointers.

**Say "start a run" and the procedure is [`RUNS.md`](RUNS.md).**

| | |
|---|---|
| [`RUNS.md`](RUNS.md) | How a run is conducted. The loop, the three input streams, the rules |
| [`TODO.md`](TODO.md) | The live list — open, blocked, awaiting the user |
| [`research.md`](research.md) | Deep dives too big for one run, parked with enough context to start cold |
| [`optimisation.md`](optimisation.md) | Squeezing the daemon — resource governance, build profile, VBV. Where the knobs live |
| `memory/` | What previous runs learned, by subject |

## Start here for numbers

- [`memory/latency/measurements.md`](memory/latency/measurements.md) — **every configuration
  measured, with achieved fps, jitter, drops and stalls**, plus a ⚑ findings section at the
  top and the known-good baseline to A/B against.
- [`memory/shared/metrics.md`](memory/shared/metrics.md) — **what to collect and what each
  metric separates**, with a symptom → reading diagnostic table.

---

## Run types

Work splits into three kinds of run. Each has its own memory directory; each has its own
rhythm. A run usually stays in one lane — say which lane at the start.

### 🔴 Latency / streaming — `memory/latency/`

Making the stream faster and steadier. Driven by tracing, browser stats and user reports of
stutter, lag or artefacts. The most measurement-heavy lane: nothing is fixed here without a
number before and after.

| File | Holds |
|---|---|
| [`measurements.md`](memory/latency/measurements.md) | Results per configuration — fps, jitter, drops, stalls — and the ⚑ findings list |
| [`pipeline.md`](memory/latency/pipeline.md) | Where the time actually goes, and the measured local maximum |
| [`transport.md`](memory/latency/transport.md) | webrtc-rs, relay, ICE — including what has been ruled out |
| [`encoder.md`](memory/latency/encoder.md) | Bitrate, VBV, keyframes, presets |
| [`bandwidth.md`](memory/latency/bandwidth.md) | How much the stream needs — pixel rate, bits/pixel, why fps is the lever |

### 🟢 Client UI — `memory/ui/`

The Dioxus/WASM client: layout, controls, touch, the JS bridge. Driven almost entirely by
user reports, because "it blocks where I need to tap" is not visible in any log. Verification
is a human looking at it — byte checks prove nothing here.

| File | Holds |
|---|---|
| [`client.md`](memory/ui/client.md) | Layout rules learned the hard way, bridge structure, state |

### 🔵 Compositor core — `memory/compositor/`

wado as a Wayland compositor: protocols, windows, input synthesis, session lifetime. Driven
by the feature list and by applications behaving wrongly inside the session.

| File | Holds |
|---|---|
| [`lifecycle.md`](memory/compositor/lifecycle.md) | Sessions, launched processes, signals, the PTY |
| [`wayland.md`](memory/compositor/wayland.md) | Protocols implemented, scaling, outputs, input |
| [`wayland-protocols.md`](memory/compositor/wayland-protocols.md) | Full protocol inventory — implemented, missing and worth it, ranked for phone/touch |

### Cross-cutting — `memory/shared/`

True in every lane. **`environment.md` is read first, every run, whatever the lane.**

| File | Holds |
|---|---|
| [`environment.md`](memory/shared/environment.md) | Operational traps — RTK, release builds, the rig |
| [`metrics.md`](memory/shared/metrics.md) | What to measure, what each metric separates, the diagnostic table |
| [`verification.md`](memory/shared/verification.md) | How measurements have lied here, and the guards |
| [`build-deploy.md`](memory/shared/build-deploy.md) | Cargo, Nix, Pages, releases |

---

## Where things live

| | |
|---|---|
| Repo direction and invariants | `CLAUDE.md` (repo root) |
| Project milestones | `TODO.md` (repo root) |
| Architecture + Decision Log | `WADO_PLAN.md` (repo root) |
| Input-handling war stories | `CHALLENGES.md` (repo root) |

⚠️ Those four are **gitignored** — edits stay on this machine and never reach a clone.

---

## Maintaining memory

- **Concise.** If it does not change a future decision, leave it out.
- **Corrective.** Record withdrawn claims explicitly. A wrong conclusion left standing costs
  more than the bug did.
- **Supersede, don't append.** Rewrite a stale entry; don't stack corrections on it.
- **Verify before acting.** An entry naming a file, flag or function is a lead, not a fact.
- Update memory *and* `TODO.md` before a run ends.

---

## The `plan` branch

`plan/` is gitignored on `main` — it lives only on the orphan branch **`plan`**, which
contains nothing else. No code, no merges, no relation to `main`'s history.

**Commit updated plans to `plan` separately, after any change to this folder.** Never fold
plan edits into a code commit on `main`; never merge `plan` into `main` or the reverse.

From the repo root, with `main` still checked out:

```sh
plan/commit.sh "what changed"
```

It writes the commit straight to `refs/heads/plan` through a side index, so the working tree
and `main`'s index are untouched. Push with `git push origin plan`.
