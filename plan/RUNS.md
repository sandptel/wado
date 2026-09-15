# RUNS.md — how a wado run is conducted

A **run** is one iteration of the loop we work in. Say *"start a run"* and this is the
procedure. It is a handoff document: a session that has lost its context should be able to
read this plus `plan/memory/` and pick the loop back up without asking what happened before.

Not in git (`plan/` is ignored) — this is our working surface, not a project artifact.

---

## The loop

```
orient → restore → gather → measure → fix → verify → commit+push → rehost → observe → branch
```

It is a loop, not a checklist. A run usually ends because something is confirmed working
and the next thing is unclear — not because the list is empty.

### 1. Orient (always first, always cheap)

Read, in this order:

1. `plan/memory/00-environment.md` — the operational traps. Reading this late costs an hour.
2. `plan/TODO.md` — what is open, what is blocked, what is awaiting the user.
3. The rest of `plan/memory/` as the subject demands.
4. `CLAUDE.md` → `TODO.md` → `WADO_PLAN.md` in the repo, for direction and invariants.

Never re-derive a fact that memory already holds. Never trust a memory that names a file,
flag or function without checking it still exists.

### 2. Restore the rig

A run needs the infrastructure up. Check and restart what is missing:

- `wado-relay` — the rendezvous.
- `cloudflared` — the public tunnel fronting the relay.
- `wado` daemon — **release build, always** (see memory 00; this is the single most
  expensive mistake available).
- A `Monitor` on the daemon log, filtered to anomalies only.

State it plainly if something was already down — never imply a restart happened when it did
not, and never report the rig healthy without having looked.

### 3. Gather — the three input streams

A run is driven by three sources, and all three get read every time:

| Stream | What it is | How it is treated |
|---|---|---|
| **The TODO list** | `plan/TODO.md` — carried work | The default work when nothing else is urgent |
| **The user's reports** | "1080p balanced is unusable", "there is no logs opening" | **Highest priority.** A user report outranks the list |
| **Own tracing events** | Monitor events, daemon log, browser stats | Where anomalies are found that nobody reported |

The third stream is the one that earns the run. A user reports what they can see; the log
shows the cause, and often shows a second fault nobody has noticed yet.

### 4. Measure before fixing

Non-negotiable, and the rule most often violated under time pressure.

- Reproduce the symptom, or find it in the log, **before** changing anything.
- State the hypothesis, then the measurement that would **refute** it.
- A fix justified by reasoning alone is labelled as such and never reported as verified.
- **Check the instrument before trusting the number.** A threshold-triggered log censors its
  own sample; an exit code from a pipeline is the last stage's. Both have lied in this
  project (memory 07).
- When the number contradicts the hypothesis, report the number.

### 5. Fix

- Smallest diff that holds. Stdlib before a dependency; an existing dependency before a new
  one; deletion before addition.
- One concern per commit, so a regression can be bisected.
- Non-trivial logic leaves one runnable check behind — **and the check is confirmed to fail
  against the old code**, or it proves nothing.
- Mark deliberate simplifications `ponytail:` with the ceiling named and the upgrade path.

### 6. Verify — the real thing, not a proxy

Layers, each of which has given a false pass here at least once:

1. `cargo test` — the logic.
2. Build the artifact that actually ships (**release**).
3. Confirm the code reached the deployed artifact — the right one. The bridge JS is
   `include_str!`'d into Rust, so it lives in the **`.wasm`**, not the JS shim.
4. **Byte verification says the code arrived. It says nothing about whether it renders,
   mounts or runs.** Only the user's eyes close that gap — so ask.

### 7. Commit, push, rehost

Standing authorisation: commit and push after each significant step, with an explanatory
message. Then rebuild and restart whatever runs the changed code, and say so.

Commit messages explain **why**, including what was rejected and what the known ceiling is.
Record withdrawn claims explicitly — a wrong conclusion left standing costs the next session
more than the bug did.

### 8. Observe, then branch

After the rig is back up, watch. When the reported fault is fixed and the log is quiet, the
run branches down into optimisation:

- Pick the **largest measured** cost still on the table, not the most interesting one.
- Form the hypothesis and the refuting measurement first.
- Prefer an experiment that can be reverted in one commit.
- A speculative tune with no measured problem behind it is **not** an optimisation — it is a
  regression waiting for a user to find it. (Worker-thread capping was rejected on exactly
  this ground: upstream data said it should help, our own measurement said there was nothing
  left to fix.)

### 9. Close the run

Before the run ends, always:

- Update `plan/TODO.md` — what moved, what is now blocked, what needs the user.
- Update `plan/memory/` — **concise, corrective, compass-shaped**. Findings that change what
  a future session would do. Delete what has been superseded rather than appending to it.
- Tell the user what is unverified and what you need from them.

---

## Experimental runs

For a change whose effect is uncertain:

1. Write down the metric and the current value **before** touching anything.
2. Change **one** thing.
3. Rebuild, restart, re-measure the same metric the same way.
4. Keep it only if the number moved. Revert quietly if not — and record the negative result
   in memory, because a disproved idea is worth as much as a fix and will otherwise be
   retried.

A/B against the **same** configuration. Field data across different resolutions, bitrates and
networks is not a comparison; it is noise with opinions.

---

## Standing rules

- The user's report outranks the plan.
- Ambiguity that changes the work becomes a question. Ambiguity that does not gets a stated
  assumption and the work continues.
- Never report something as working that a human has not seen work.
- Correct your own earlier claims plainly and move on. Mark them **withdrawn** in memory.
- Never kill a process by loose pattern match — match the exact pid or process name. A
  `pgrep -f` pattern matches the shell running it, and that shell has been killed this way.
- Destructive or outward-facing actions (releases, tags, force pushes) get confirmed unless
  already authorised.
