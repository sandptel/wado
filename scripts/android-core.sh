#!/usr/bin/env bash
# Builds the Rust core (crates/mobile-core) for the Android app: the .so for each ABI into
# android/app/src/main/jniLibs/, and its Kotlin bindings (UniFFI) into
# android/app/build/generated/uniffi/. Both are build outputs, never committed.
#
#   nix develop .#android -c scripts/android-core.sh
#
# Android's std comes from rustup, in its own home (~/.cache/wado/rustup), at the dev shell's
# rustc version (flake.nix). If that toolchain ever fails to link ("ld-wrapper.sh: No such file"),
# the store path it was patched against was collected: delete ~/.cache/wado/rustup and rerun.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
: "${ANDROID_NDK_HOME:?run inside: nix develop .#android (or set ANDROID_NDK_HOME)}"
export CARGO_TARGET_DIR=${CARGO_TARGET_DIR:-$ROOT/target}
TARGETS=(aarch64-linux-android x86_64-linux-android)
ABIS=(arm64-v8a x86_64)

# Outside the Nix shell (CI), the cargo on PATH is used as it is: it must have both targets.
if [ -n "${WADO_RUSTUP:-}" ]; then
  export RUSTUP_HOME=${RUSTUP_HOME_WADO:-$HOME/.cache/wado/rustup}
  TOOLCHAIN=$WADO_RUST_VERSION
  "$WADO_RUSTUP" toolchain install "$TOOLCHAIN" --profile minimal --no-self-update >/dev/null
  "$WADO_RUSTUP" target add --toolchain "$TOOLCHAIN" "${TARGETS[@]}" >/dev/null
  # That toolchain, first on PATH and named outright: the shell's own cargo/rustc (for the web
  # client) would otherwise answer, and they have no Android std.
  TC=$(dirname "$("$WADO_RUSTUP" which --toolchain "$TOOLCHAIN" rustc)")
  export PATH=$TC:$PATH RUSTC=$TC/rustc CARGO=$TC/cargo
fi

JNI=$ROOT/android/app/src/main/jniLibs
OUT=$ROOT/android/app/build/generated/uniffi
rm -rf "$JNI" "$OUT"
mkdir -p "$JNI" "$OUT"
cd "$ROOT"
# Release: the workspace profile (fat LTO, one codegen unit) makes a small, fast .so.
cargo ndk --platform 26 -o "$JNI" \
  $(printf -- '-t %s ' "${ABIS[@]}") build -p wado-mobile-core --release --lib

# Bindings from the built library itself (library mode), so they cannot drift from it.
LIB=$CARGO_TARGET_DIR/aarch64-linux-android/release/libwado_mobile_core.so
cargo run -q -p wado-mobile-core --features bindgen --bin uniffi-bindgen -- \
  generate --library "$LIB" --language kotlin --out-dir "$OUT" --no-format
ls "$JNI"/*/libwado_mobile_core.so
echo "bindings: $OUT"
