# macOS host — what the 2026-11-09 run learned

Branch `macos`. Spec `plan/superpowers/specs/2026-11-09-macos-host-design.md`, plan
`plan/superpowers/plans/2026-11-09-macos-host.md`, recon `plan/superpowers/research/2026-11-09-macos-host-recon.md`.

## Shape
- `crates/server/src/compositor.rs` is the only file that names the compositor: Linux re-exports
  `wado-compositor`, macOS `wado-session-macos` (same names; every session command answered
  "coming to macOS"). Cargo **refuses** one dependency name with different paths per target
  ("must have a single canonical source path"), hence the re-export file.
- Providers: `crates/macos/host` (`wado-host-macos`), whole crate `#![cfg(target_os = "macos")]`
  so Linux builds it empty. PTY is shared (`server/src/platform/pty.rs`, `Portable` provider on Mac).
- Wire: `ConfigState.platform` + `host_caps` (CapSet with `CapOff.later`). Per-OS "later"/"never"
  tables in `server/src/platform/mod.rs` (`mark`). `host_caps()` reprobes stale caps on a Mac so a
  grant turns input on without a restart.

## Traps
- No `openat2`/`O_PATH`/`/proc/self/fd` on macOS: `files/beneath.rs` walks components with
  `O_NOFOLLOW` from the root fd; `O_EVTONLY` stands in for `O_PATH`; `F_GETPATH` for real paths;
  `renameatx_np(RENAME_EXCL)` for `RENAME_NOREPLACE`. `/var` → `/private/var` symlink: compare
  canonical paths.
- TCC: grants follow the signature. `scripts/mac-cert.sh` makes a self-signed `wado-dev` identity
  (no trust or password needed to sign; DR pins the leaf hash, stable across rebuilds).
  `scripts/mac-app.sh --run` launches with `open` so the grants are Wado's, not the terminal's.
  A daemon run from a terminal gets the terminal's grants.
- Without Accessibility CGEvents are dropped silently → input caps Off with the pane named.
- `~/.Trash` is readable only with Full Disk Access; trashing works without it.
- MediaRemote is locked to Apple since 15.4 → AppleScript for Music/Spotify + media keys.
- `scripts/mac-e2e.mjs` (local relay + Wado.app + `client-core/tests/mac_live.rs`) — pairing,
  platform, caps, outputs, clipboard both ways, pointer move (when Accessibility is granted).
