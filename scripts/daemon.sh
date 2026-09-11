#!/usr/bin/env bash
# Build and run the wado daemon — release only, by construction.
#
# Why this exists: a debug build of the daemon does not look broken, it looks like a slow
# pipeline. 1080p stutter was chased as an encoder problem for a whole session before the
# build profile turned out to be the entire cause (release vs debug, same config: stalls
# 14 → 0, jitter buffer 19–42 ms → 12 ms flat, fps 48–60 → 60 steady).
#
# So there is one way to start the daemon and it cannot be the wrong one. `cargo run` is the
# footgun; this is the replacement.
set -euo pipefail
cd "$(dirname "$0")/.."

# Niced because building while a session is live is itself a suspected source of stalls:
# cargo saturating every core is the one condition under which they were first seen.
nice -n 19 cargo build --release -p wado "$@"

exec ./target/release/wado
