#!/usr/bin/env bash
# Builds the Android app (android/): the web client first, bundled into the APK's assets, then the
# APKs. Run inside the Android dev shell, which brings the SDK, JDK 17 and dx:
#
#   nix develop .#android -c scripts/android-build.sh            # debug + release APKs
#   nix develop .#android -c scripts/android-build.sh --debug    # debug only
#   … --no-client    reuse the client already in android/app/src/main/assets/client/
#
# Outputs: android/app/build/outputs/apk/{debug,release}/, and a copy of the debug APK at
# ~/.cache/wado/wado-<version>-debug.apk. Release is debug-signed unless a release keystore is
# set up (docs/android-release.md) — its file name then says "-unsigned-key" so it cannot be
# mistaken for a release.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
DEBUG_ONLY=0 CLIENT=1
for a in "$@"; do
  case "$a" in
    --debug) DEBUG_ONLY=1 ;;
    --no-client) CLIENT=0 ;;
    *) echo "unknown option: $a" >&2; exit 2 ;;
  esac
done
: "${ANDROID_HOME:?run inside: nix develop .#android}"

ASSETS=$ROOT/android/app/src/main/assets/client
if [ "$CLIENT" = 1 ]; then
  # Base path "/": the app serves the client at the root of its own origin
  # (https://appassets.androidplatform.net/), not under /wado/ as Pages does.
  OUT=${CARGO_TARGET_DIR:-$ROOT/target}/dx/wado-client/release/web/public
  # dx keeps every earlier build's hashed files beside the new ones; only the new ones ship.
  rm -rf "$OUT"
  (cd "$ROOT" && dx build -p wado-client --release --platform web --base-path /)
  [ -f "$OUT/index.html" ] || { echo "no client build at $OUT" >&2; exit 1; }
  rm -rf "$ASSETS"
  mkdir -p "$ASSETS"
  cp -r "$OUT"/. "$ASSETS"/
  # The service worker and its signing files are the installed web app's; the APK updates
  # its client only with itself (js/app_sw.js registers nothing there).
  rm -f "$ASSETS"/sw.js "$ASSETS"/sw-keys.js "$ASSETS"/sw-verify.js "$ASSETS"/release.json "$ASSETS"/release.sig
fi
[ -f "$ASSETS/index.html" ] || { echo "no client in $ASSETS (drop --no-client)" >&2; exit 1; }

# The Rust core (.so per ABI + its Kotlin bindings).
"$ROOT/scripts/android-core.sh"

cd "$ROOT/android"
TASKS=(test assembleDebug)
[ "$DEBUG_ONLY" = 1 ] || TASKS+=(assembleRelease)
./gradlew --no-daemon "${TASKS[@]}"

VERSION=$(sed -n 's/^version = "\(.*\)"/\1/p' "$ROOT/crates/client/Cargo.toml" | head -1)
mkdir -p "$HOME/.cache/wado"
cp app/build/outputs/apk/debug/app-debug.apk "$HOME/.cache/wado/wado-$VERSION-debug.apk"
echo "debug APK: $HOME/.cache/wado/wado-$VERSION-debug.apk"
if [ "$DEBUG_ONLY" = 0 ]; then
  REL=$(ls app/build/outputs/apk/release/*.apk | head -1)
  if [ -f keystore.properties ] || [ -n "${WADO_KEYSTORE_FILE:-}" ]; then
    echo "release APK: $ROOT/android/$REL"
  else
    cp "$REL" "$HOME/.cache/wado/wado-$VERSION-release-unsigned-key.apk"
    echo "release APK (debug-signed — no release key set up): $HOME/.cache/wado/wado-$VERSION-release-unsigned-key.apk"
  fi
fi
