#!/usr/bin/env bash
# Boots a headless Android emulator (KVM), installs the debug APK and takes screenshots — the
# app's smoke test where no phone is attached. Run inside the emulator shell:
#
#   nix develop .#android-emulator -c scripts/android-emulator.sh [apk] [screenshot dir]
#   nix develop .#android-emulator -c scripts/android-emulator.sh --tests   # instrumented tests
#
# The AVD lives in ~/.android/avd/wado.avd (made once). The emulator is left running for adb;
# stop it with `adb emu kill`.
set -euo pipefail
ROOT=$(cd "$(dirname "$0")/.." && pwd)
TESTS=0
if [ "${1:-}" = "--tests" ]; then TESTS=1; shift; fi
APK=${1:-$ROOT/android/app/build/outputs/apk/debug/app-debug.apk}
SHOTS=${2:-$ROOT/captures/android}
PKG=io.github.sandptel.wado.debug
: "${ANDROID_HOME:?run inside: nix develop .#android-emulator}"
mkdir -p "$SHOTS"

IMAGE=$(ls -d "$ANDROID_HOME"/system-images/*/google_apis/x86_64 | sort -V | tail -1)
API=$(basename "$(dirname "$(dirname "$IMAGE")")")
if [ ! -d "$HOME/.android/avd/wado.avd" ]; then
  echo no | avdmanager create avd -n wado -k "system-images;$API;google_apis;x86_64" -d pixel_7 --force
fi

if ! adb devices | grep -q '^emulator-'; then
  emulator -avd wado -no-window -no-audio -no-boot-anim -gpu swiftshader_indirect -no-snapshot \
    >"$SHOTS/emulator.log" 2>&1 &
fi
adb wait-for-device
until [ "$(adb shell getprop sys.boot_completed 2>/dev/null | tr -d '\r')" = 1 ]; do sleep 2; done
adb shell settings put global window_animation_scale 1
if [ "$TESTS" = 1 ]; then
  # The instrumented tests (androidTest): NavRestoreTest, HomeUiTest, PairUiTest.
  cd "$ROOT/android" && exec ./gradlew --no-daemon connectedDebugAndroidTest
fi
adb install -r -g "$APK"

shot() { sleep "${2:-4}"; adb exec-out screencap -p >"$SHOTS/$1.png"; echo "screenshot: $SHOTS/$1.png"; }
adb shell am start -W -n "$PKG/io.github.sandptel.wado.MainActivity" >/dev/null
shot 01-home 6
# The share sheet, from a link shared by another app.
adb shell am start -W -a android.intent.action.SEND -t text/plain \
  --es android.intent.extra.TEXT "https://example.com/hello" -n "$PKG/io.github.sandptel.wado.MainActivity" >/dev/null
shot 02-share 3
adb shell input keyevent KEYCODE_BACK
# The bundled client, through a shortcut's door link.
adb shell am start -W -a android.intent.action.VIEW -d "wado://go/files" -n "$PKG/io.github.sandptel.wado.MainActivity" >/dev/null
shot 03-client 8
adb shell input keyevent KEYCODE_BACK
shot 04-back 3
adb logcat -d -s wado-page:* chromium:* AndroidRuntime:E >"$SHOTS/logcat.txt" || true
echo "logcat: $SHOTS/logcat.txt"
