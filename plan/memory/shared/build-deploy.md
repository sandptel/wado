# build and deploy

## Builds

- **Always run the daemon from `./target/release/wado`.** See `environment.md`.
- `cargo fmt` is **not clean at baseline** — format only the files you touched, after
  checking that file's baseline. Repo-wide formatting buries the real diff.
- The dev shell and the host are different worlds. Libraries that must agree have to be made
  to agree explicitly; alignment that happens to hold today is not a fix.

## Nix

`flake.nix` is **in the repo** as of 2026-09-11 (it previously lived in the parent directory,
so the dev shell could not be obtained by cloning).

- `nix build github:sandptel/wado` → `./result/bin/{wado,wado-relay}`
- `nix run github:sandptel/wado` / `#relay`
- Smithay is a git dep, so its hash is stated in `cargoLock.outputHashes`. It changes when
  the pinned rev changes; the build prints the correct one on mismatch.
- Binaries are **wrapped** with `LD_LIBRARY_PATH` and `LIBVA_DRIVERS_PATH`: the GL/VA-API
  stack is dlopened by name at session start, so without the wrapper the package installs
  fine and fails only when someone connects.
- `doCheck = false` — the suite spawns processes and claims a Wayland socket, wanting an
  `XDG_RUNTIME_DIR` the sandbox lacks.
- The source filter drops `target/` (a 287 MB debug binary; copying it looks like a hang).
- Locally needs `--option sandbox false` (see `environment.md`).
- `LIBVA_DRIVERS_PATH` points at the shell's **own** mesa on purpose: libva probes for
  `__vaDriverInit_<major>_<minor>` scanning *downward* from its own version, so a driver
  built against a newer libva is invisible. That was the VA-API `-5` failure.

## GitHub Pages

Client deploys on every push to main via `.github/workflows/pages.yml`.
`--base-path wado` is a **CLI flag in the workflow only** — putting `base_path` in
`Dioxus.toml` breaks local `dx serve` (root 404s).

Verification: wait on the **specific `headSha`**, then grep the deployed
`wado-client_bg-*.wasm` (not the JS shim). See `verification.md`.

Transient CI failures seen: `deploy-pages` 403 on `ListArtifacts` — re-run the job.

## Releases

`v0.0.1` = the measured latency local maximum, `CHANGELOG.md` holds the numbers and caveats.
Crate versions track the tag.

**No binaries are attached deliberately** — the daemon links the dev shell's FFmpeg 8.x ABI
and VA-API stack, so a binary built here would not run elsewhere. Nix is the distribution
route. (`v0.0.1` predates the flake, so the tag itself cannot be `nix build`-ed — unresolved.)

## Licensing

wado is **AGPL-3.0-only**; every crate carries the SPDX field. Keep new crates AGPL.
`gst-wayland-display` and Wolf are MIT — reuse freely. **RustDesk is AGPL: study its design,
never its source.** `portable-pty` is MIT (compatible).


---

## The daemon is release-only, and there is now one way to start it

`scripts/daemon.sh` builds `--release` and execs the result. Use it. `cargo run` is the
footgun it replaces: a debug daemon does not look broken, it looks like a slow pipeline, and
that cost a whole session once (release vs debug, same config: stalls 14 → 0, jitter buffer
19–42 ms → 12 ms flat, fps 48–60 → 60 steady).

The script nices the build, because cargo saturating every core while a session is live is
itself a suspected source of stalls — never build unniced while the user is testing.

**Release profile** (workspace `Cargo.toml`): `lto = "fat"`, `codegen-units = 1`,
`debug = 1` (line tables, so `perf` and backtraces name real functions at no runtime cost).

⚠️ **`panic = "unwind"` is pinned explicitly and must stay.** `panic = "abort"` reads like a
free win and would silently delete the per-session panic containment that keeps the server
alive through a broken session — a crash-isolation invariant in `CLAUDE.md`.

**Restarting the live daemon** without losing its environment: it inherits a direnv shell, so
copy `/proc/<pid>/environ` (NUL-separated) and re-exec with exactly that env and cwd. SIGTERM
is graceful now — the signalfd handler stops the session and releases the render node before
exit, so wait for the process to go rather than following up with SIGKILL.

## ⛔ A reload in the ten minutes after a deploy gets the OLD client

`2026-09-20`. GitHub Pages serves `index.html` with `cache-control: max-age=600`, and that file
is the only unhashed thing in the build — it names the hashed JS, which names the hashed wasm.
So for ten minutes after a push, a phone that reloads re-reads its cached index and re-fetches
**the previous build**, with nothing anywhere saying so: CI is green, the three-hop hash check
passes against the server, and the device is still running last build's code.

Cost here: four reload cycles and a wrong hypothesis about which build was under test.

The tell is a feature you just shipped being absent on the device while `curl` proves it is in
the deployed wasm. Two ways past it, and the first is instant:

```
https://sandptel.github.io/wado/?v=<anything>   # a different URL, so a different cache entry
```

or wait out the 600 s. **Verify the deploy against the device, not against the origin** — the
origin was never the thing in doubt.
