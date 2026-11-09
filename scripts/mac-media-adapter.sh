#!/usr/bin/env sh
# Build mediaremote-adapter (github.com/ungive/mediaremote-adapter, BSD-3-Clause) into
# target/media-adapter/: its Perl script and MediaRemoteAdapter.framework. Apple's own
# /usr/bin/perl may read "now playing" since macOS 15.4, so wado runs the script with it
# (crates/macos/host/src/media.rs). Needs git, cmake and the Xcode command-line tools.
# Built once per tag; scripts/mac-app.sh bundles the result.
set -eu
cd "$(git rev-parse --show-toplevel)"
TAG=v0.7.7
SRC=target/mediaremote-adapter-$TAG
OUT=target/media-adapter
if [ -d "$OUT/MediaRemoteAdapter.framework" ] && [ "$(cat "$OUT/TAG" 2>/dev/null)" = "$TAG" ]; then
  exit 0
fi
[ -d "$SRC" ] || git -c advice.detachedHead=false clone -q --depth 1 --branch "$TAG" https://github.com/ungive/mediaremote-adapter.git "$SRC"
cmake -S "$SRC" -B "$SRC/build" -DCMAKE_BUILD_TYPE=Release >/dev/null
cmake --build "$SRC/build" --target MediaRemoteAdapter >/dev/null
rm -rf "$OUT"
mkdir -p "$OUT"
cp "$SRC/bin/mediaremote-adapter.pl" "$SRC/LICENSE" "$OUT/"
cp -R "$SRC/build/MediaRemoteAdapter.framework" "$OUT/"
echo "$TAG" > "$OUT/TAG"
