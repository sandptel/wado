# Release signing — how an installed wado takes updates

wado installs as an app (a PWA: `crates/client/public/manifest.webmanifest`). Once installed, it
opens from its own service worker (`public/sw.js`), which serves the exact files of one release —
offline too — and, once a release key is set up, adopts a newer release **only when it is signed
with your offline release key**. Every push to `main` still builds and deploys the client to
GitHub Pages.

## Two modes, chosen by `sw-keys.js`

| `sw-keys.js` | installed apps | Diagnostics |
|---|---|---|
| **no key** (as shipped) | take every new release, signed or not, newer or older — like any web app, but still downloaded whole and used from the next launch | *Updates aren't signed yet — set up a release key (docs/release-signing.md)* |
| **a key** | take only a release signed with a listed key, every file matching, not older than the one in use; refuse the rest | `wado 0.0.3 · signed ✓ · updated 30 Oct`, or *An unsigned update was refused* |

Pinning switches on by itself: the release whose `sw-keys.js` first lists a key is taken like any
other (signing is still off), and from the next launch the worker carrying the key is in charge.
A DEV key (`dev: true`) counts as a key only on localhost, never on a deployed address.

## What a release is

| file | written by | holds |
|---|---|---|
| `release.json` | CI (`release-sign.mjs manifest`) | `{app, version, commit, built, files: {path: sha256}}` — every file of the build |
| `release.sig` | you, offline (`release-sign.mjs sign`) | base64 Ed25519 signature over release.json's exact bytes |
| `sw-keys.js` | you, in the repo | the public keys the app trusts |

With a key set up, the worker, on each launch (and when the app comes back to the front, at most
every 10 min):

1. fetches `release.json` and `release.sig`;
2. verifies the signature against a key in `sw-keys.js`, and that the release is not older than
   the one in use (no replaying an old signed release);
3. downloads every file and checks each against its hash;
4. only then marks it **pending** — and the page shows *Update ready — reopen to use it*;
5. takes it into use on the next launch, when no other wado window is open. Never mid-session.

Anything unsigned, signed by an unknown key, malformed, older, or with a file that does not match
is **refused**: the app keeps the version it has, writes the reason to the log panel and the
browser console, and Diagnostics says *An unsigned update was refused* (or which check failed).

Diagnostics → the first card always shows what is in use: `wado 0.0.3 · signed ✓ · updated 30 Oct`
and the signing key's short name (`release key 3f9a 21c0`).

## Trust on first use

The **first** install trusts whatever the server hands it, signed or not — there is nothing yet
to check it against except the key the same server just sent. Diagnostics says whether that first
version was signed. From then on, once a key is set up, only signed updates are taken. Until a key
is set up, none of this protects anything: every release is taken.

## What this protects, and what it does not

- **Protects** an installed app from a client bundle changed on the hosting side (Pages, its CDN,
  a hijacked deploy) or a build you did not approve: none of those carry your signature.
- **Does not protect a first visit** (trust on first use, above), nor a browser where the app was
  never installed.
- **Does not protect against a replaced service worker script.** Browsers re-fetch `sw.js` (and
  `sw-keys.js`, `sw-verify.js`) themselves, at least daily, and install a changed copy no matter
  what the running worker thinks. Someone who can change what Pages serves can therefore ship a
  worker that skips the check; it takes effect on the next launch. No web API lets a worker pin
  itself. The signed-update check raises the bar for tampering with the *client*; it is not a
  guarantee against a compromised host. The key's short name in Diagnostics changing unexpectedly
  is the visible sign.
- **Does not protect against CI building something you did not mean to**: you sign what CI built
  (below). Check the commit the script prints is one you pushed.

## The signing flow, and why this one

**Chosen: sign the CI artifact after download, then re-deploy that same artifact with its
signature.** The alternative — build and sign locally, deploy the local tree — needs either a
byte-reproducible build (so CI's output matches your hashes; Rust/WASM builds are not reliably
reproducible across machines) or a second upload path for a locally built tree. Signing the
artifact CI already built needs neither: the bytes you hash are the bytes Pages serves, and the
current workflow stays as it is (push to `main` builds and deploys). The private key never touches
CI; only the 88-character signature goes back, as a workflow input.

### One time: make the key

On the machine that will hold the key (ideally offline):

```sh
node scripts/release-sign.mjs keygen ~/wado-release-key.pem
```

It writes the private key (mode 0600, never overwrites) and prints a line like

```js
{ key: "u0Q…=", note: "release key, made 2026-10-30" },
```

Paste that line into `crates/client/public/sw-keys.js`, commit and push. Until then signing is
off and installed apps take every update unsigned. That push's build is taken unsigned like the
ones before it; from the launch after, installed apps take only signed releases — **so sign every
release from that one on**, or they stay on it and say *An unsigned update was refused*.

Back the private key up offline. Never commit it, never give it to CI.

### Each release

1. Push to `main`. The `pages` workflow builds, writes `release.json` and deploys (unsigned).
2. Sign that run's artifact:

   ```sh
   gh run list --workflow pages.yml -L 5          # the newest successful run id
   scripts/release-sign-run.sh <run-id> ~/wado-release-key.pem
   ```

   It downloads the artifact, checks every hash against `release.json`, prints the version and
   the commit CI built — **check that commit is yours** — signs, and prints:

   ```sh
   gh workflow run pages.yml -f run_id=<run-id> -f signature=<base64>
   ```

3. Run that. The `sign` job downloads the same artifact, adds `release.sig`, verifies it against
   the tree's `sw-keys.js` (it refuses to deploy a signature that does not verify), and deploys.
   Installed apps pick it up on their next launch.

Sign the **newest** run: deploying an older run's signature puts Pages back on that older build.
Artifacts are kept 14 days.

Air-gapped signing works the same way by hand: `gh run download <run-id> -n github-pages`, carry
`artifact.tar` to the offline machine, `tar -xf` it into a folder, `node scripts/release-sign.mjs
sign <folder> <key>`, and carry the printed signature back.

### Rotating the key

1. `keygen` a new key. Add its line to `sw-keys.js` **next to** the old one. Push, and sign that
   release with the **old** key — installed apps trust it, and receive the new key list with it.
2. Sign the following releases with the new key.
3. Once every device you care about has launched a release from step 1, remove the old line.

**A lost or leaked key:** remove it from `sw-keys.js` and add a new one. A leaked key can sign
updates that installed apps accept until they have taken a release without it. A lost key with
no second key listed cannot be recovered from: installed apps keep their version until the person
clears the site's data (browser settings → site settings → wado → clear) and installs again.

## Local testing (DEV key)

```sh
dx build -p wado-client --release --platform web --base-path wado
node scripts/app-e2e.mjs --shots /tmp/wado-shots
```

`app-e2e.mjs` copies the build into throwaway trees and signs them with
`node scripts/release-sign.mjs dev <tree>`, which makes a **DEV key** at
`target/release-dev-key/dev-key.pem` (git-ignored, never committed), adds it to that tree's
`sw-keys.js` marked `dev: true`, and signs. The worker honours a `dev` key **only on localhost**,
so a DEV key that reached a deploy would still sign nothing there. The committed `sw-keys.js`
ships no DEV key (`node scripts/sw-check.mjs` checks).

A `dx serve` session has no `release.json`; the worker sees that and pins nothing.

## Files

| file | job |
|---|---|
| `crates/client/public/sw.js` | serves the pinned release, adopts signed ones on next launch |
| `crates/client/public/sw-verify.js` | the checks: signature, shape, age, file hashes (pure) |
| `crates/client/public/sw-keys.js` | the trusted public keys |
| `crates/client/src/js/app_sw.js` | registers the worker, reports to the UI and the log |
| `scripts/release-sign.mjs` | keygen, manifest, check, sign, verify, dev |
| `scripts/release-sign-run.sh` | download a Pages run's artifact and sign it |
| `scripts/sw-check.mjs` | unit checks for `sw-verify.js` |
| `scripts/app-e2e.mjs` | the whole flow in headless Chrome |
