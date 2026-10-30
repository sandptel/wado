# Signing the Android app for release

`scripts/android-build.sh` builds a **debug-signed** release APK until a release key is set up.
That APK works, but Android ties every future update to the key an app was first installed with,
so a phone that installed a debug-signed build has to uninstall (losing its pairing) to take a
properly signed one. Set the key up before handing the app to anyone.

The key is **yours, held offline, and never committed** (`android/.gitignore` refuses
`keystore.properties`, `*.jks`, `*.keystore`). Lose it and no update can reach the installed
app again; leak it and anyone can ship an "update" a phone will accept.

## 1. Make the keystore, once

In the Android dev shell (`nix develop .#android`), somewhere outside the repository:

```sh
keytool -genkeypair -v -keystore ~/keys/wado-release.jks -alias wado \
  -keyalg RSA -keysize 4096 -validity 36500
```

Back it up (two places, at least one offline). Note its SHA-256 fingerprint — it is what users
and F-Droid-style verifiers compare against:

```sh
keytool -list -v -keystore ~/keys/wado-release.jks -alias wado | grep SHA256
```

## 2. Point the build at it

Either `android/keystore.properties` (ignored by git):

```properties
storeFile=/home/you/keys/wado-release.jks
storePassword=…
keyAlias=wado
keyPassword=…
```

or the environment, for a one-off build that leaves nothing on disk:

```sh
WADO_KEYSTORE_FILE=~/keys/wado-release.jks WADO_KEYSTORE_PASSWORD=… \
WADO_KEY_ALIAS=wado WADO_KEY_PASSWORD=… scripts/android-build.sh
```

`android/app/build.gradle.kts` uses the release key when either is present and the debug key
otherwise. The script says which it used, and a debug-signed release APK is copied with
`-release-unsigned-key` in its name so it cannot pass for a release.

## 3. Verify what you are shipping

```sh
apksigner verify --print-certs android/app/build/outputs/apk/release/app-release.apk
```

The certificate digest must be the fingerprint from step 1.

## CI

The `android` job in `.github/workflows/android.yml` builds only the **debug** APK, as an
artifact, with no secrets. Release signing stays on your machine; nothing about the key reaches
CI.
