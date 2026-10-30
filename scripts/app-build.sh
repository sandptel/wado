#!/usr/bin/env bash
# Build the Flutter app (app/): its Rust bridge (crates/app-bridge) for the target, then Flutter.
#   nix develop .#app -c scripts/app-build.sh --android [--release]   → ~/Downloads/wado-next-*.apk
#   nix develop .#app -c scripts/app-build.sh --linux [--release]     → app/build/linux/x64/*/bundle
# Run the Linux build with LD_LIBRARY_PATH=$WADO_APP_LIBS (flutter_webrtc's libwebrtc needs them).
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
TARGET=android MODE=debug
for a in "$@"; do
  case $a in
    --android) TARGET=android ;;
    --linux) TARGET=linux ;;
    --release) MODE=release ;;
    --debug) MODE=debug ;;
    *) echo "unknown: $a" >&2; exit 2 ;;
  esac
done
cd "$ROOT"
if [ "$TARGET" = linux ]; then
  cargo build -p wado-app-bridge $([ $MODE = release ] && echo --release)
  (cd app && flutter build linux --$MODE)
  exit 0
fi
: "${ANDROID_NDK_HOME:?run inside: nix develop .#app}"
# The Android std comes from rustup at this shell's rustc version (as scripts/android-core.sh).
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target-android}
TARGETS=(aarch64-linux-android x86_64-linux-android)
ABIS=(arm64-v8a x86_64)
if [ -n "${WADO_RUSTUP:-}" ]; then
  export RUSTUP_HOME=${RUSTUP_HOME_WADO:-$HOME/.cache/wado/rustup}
  "$WADO_RUSTUP" toolchain install "$WADO_RUST_VERSION" --profile minimal --no-self-update >/dev/null
  "$WADO_RUSTUP" target add --toolchain "$WADO_RUST_VERSION" "${TARGETS[@]}" >/dev/null
  TC=$(dirname "$("$WADO_RUSTUP" which --toolchain "$WADO_RUST_VERSION" rustc)")
  export PATH=$TC:$PATH RUSTC=$TC/rustc CARGO=$TC/cargo
fi
JNI=$ROOT/app/android/app/src/main/jniLibs
rm -rf "$JNI" && mkdir -p "$JNI"
cargo ndk --platform 26 -o "$JNI" $(printf -- '-t %s ' "${ABIS[@]}") build -p wado-app-bridge --release --lib
(cd app && flutter build apk --$MODE --split-per-abi)
V=$(grep '^version:' app/pubspec.yaml | awk '{print $2}' | cut -d+ -f1)
OUT=$HOME/Downloads/wado-next-$V-$MODE.apk
cp app/build/app/outputs/flutter-apk/app-arm64-v8a-$MODE.apk "$OUT"
ls -la "$OUT"
