#!/usr/bin/env sh
# Build Wado.app (target/Wado.app): the release daemon in a minimal, menu-bar-less bundle,
# signed with the `wado-dev` identity (scripts/mac-cert.sh) so its privacy grants survive
# rebuilds. `--run`: restart it against the configured relay, logging to /tmp/wado-rig/mac.log.
# Launched with `open`, so macOS asks for — and remembers — grants for Wado, not the terminal.
set -eu
cd "$(git rev-parse --show-toplevel)"
cargo build --release -p wado
# The now-playing helper (crates/macos/host/src/nowplaying.rs), from source.
LIB=target/release/libwado-nowplaying.dylib
clang -dynamiclib -fobjc-arc -O2 -Wall -framework Foundation -o "$LIB" crates/macos/host/nowplaying-helper/nowplaying.m
APP=target/Wado.app
VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' crates/server/Cargo.toml | head -1)
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp target/release/wado "$APP/Contents/MacOS/wado"
cp "$LIB" "$APP/Contents/Resources/"
sed "s/VERSION/$VERSION/g" packaging/macos/Info.plist > "$APP/Contents/Info.plist"
scripts/mac-cert.sh >/dev/null
# Inside out: the nested library first, then the app that seals it.
codesign -f -s wado-dev "$APP/Contents/Resources/libwado-nowplaying.dylib"
codesign -f -s wado-dev --identifier io.github.sandptel.wado "$APP"
codesign -dv "$APP" 2>&1 | grep -E "Identifier|Authority" || true
if [ "${1:-}" = "--run" ]; then
  pkill -x wado 2>/dev/null && sleep 1 || true
  mkdir -p /tmp/wado-rig
  open -n -a "$PWD/$APP" --stdout /tmp/wado-rig/mac.log --stderr /tmp/wado-rig/mac.log --args daemon
  echo "Wado.app started — log: /tmp/wado-rig/mac.log"
fi
